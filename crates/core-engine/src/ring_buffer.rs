use std::ffi::c_void;
use std::time::{Duration, Instant};
use tracing::{debug, warn};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Default capacity for the keyboard ring buffer.
pub const DEFAULT_BUFFER_CAPACITY: usize = 32;

/// Default timeout for keyboard inactivity before clearing buffer.
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(5);

/// A secure fixed-capacity ring buffer for intercepted keystrokes.
///
/// Features:
/// 1. Uses `zeroize` to securely erase memory upon clearing or dropping.
/// 2. Calls `VirtualLock` to lock the memory page into physical RAM,
///    preventing pagefile swapping.
/// 3. Tracks activity timestamps and automatically clears buffer when idle
///    exceeds the configured timeout.
#[derive(Debug)]
pub struct SecureRingBuffer {
    data: Vec<char>,
    capacity: usize,
    head: usize,
    len: usize,
    last_activity: Instant,
    idle_timeout: Duration,
    locked_in_memory: bool,
}

impl SecureRingBuffer {
    /// Creates a new `SecureRingBuffer` with default capacity (32) and idle timeout (5s).
    pub fn new() -> Self {
        Self::with_capacity_and_timeout(DEFAULT_BUFFER_CAPACITY, DEFAULT_IDLE_TIMEOUT)
    }

    /// Creates a new `SecureRingBuffer` with specified capacity and timeout.
    pub fn with_capacity_and_timeout(capacity: usize, idle_timeout: Duration) -> Self {
        let cap = if capacity == 0 { DEFAULT_BUFFER_CAPACITY } else { capacity };
        let mut data = vec!['\0'; cap];

        let locked = Self::lock_memory(&mut data);

        Self {
            data,
            capacity: cap,
            head: 0,
            len: 0,
            last_activity: Instant::now(),
            idle_timeout,
            locked_in_memory: locked,
        }
    }

    /// Locks the underlying buffer in physical RAM using Windows VirtualLock.
    fn lock_memory(data: &mut [char]) -> bool {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::System::Memory::VirtualLock;
            let ptr = data.as_mut_ptr() as *mut c_void;
            let bytes = std::mem::size_of_val(data);
            unsafe {
                if let Err(e) = VirtualLock(ptr, bytes) {
                    warn!("VirtualLock failed (continuing without page-lock): {:?}", e);
                    false
                } else {
                    debug!("VirtualLock successfully locked {} bytes into RAM", bytes);
                    true
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }

    /// Unlocks the underlying buffer memory using Windows VirtualUnlock.
    fn unlock_memory(&mut self) {
        #[cfg(target_os = "windows")]
        if self.locked_in_memory {
            use windows::Win32::System::Memory::VirtualUnlock;
            let ptr = self.data.as_mut_ptr() as *mut c_void;
            let bytes = std::mem::size_of_val(&self.data[..]);
            unsafe {
                let _ = VirtualUnlock(ptr, bytes);
            }
            self.locked_in_memory = false;
        }
    }

    /// Returns true if the memory page was locked in RAM via VirtualLock.
    pub fn is_locked_in_memory(&self) -> bool {
        self.locked_in_memory
    }

    /// Appends a character to the ring buffer.
    /// If buffer has exceeded idle timeout, it is cleared before inserting.
    pub fn push(&mut self, ch: char) {
        self.check_timeout_and_clear();

        if self.len < self.capacity {
            let idx = (self.head + self.len) % self.capacity;
            self.data[idx] = ch;
            self.len += 1;
        } else {
            // Buffer full: overwrite oldest character and advance head
            self.data[self.head] = ch;
            self.head = (self.head + 1) % self.capacity;
        }

        self.last_activity = Instant::now();
    }

    /// Removes the most recently added character (Backspace support).
    pub fn pop_back(&mut self) -> Option<char> {
        self.check_timeout_and_clear();

        if self.len == 0 {
            return None;
        }

        let idx = (self.head + self.len - 1) % self.capacity;
        let removed = self.data[idx];
        self.data[idx].zeroize();
        self.len -= 1;
        self.last_activity = Instant::now();
        Some(removed)
    }

    /// Checks if the buffer has been idle longer than the configured timeout,
    /// and zeroes it out if expired.
    pub fn check_timeout_and_clear(&mut self) -> bool {
        if self.last_activity.elapsed() >= self.idle_timeout && self.len > 0 {
            self.clear();
            true
        } else {
            false
        }
    }

    /// Returns the current string in FIFO order.
    pub fn as_string(&mut self) -> String {
        self.check_timeout_and_clear();
        let mut s = String::with_capacity(self.len);
        for i in 0..self.len {
            let idx = (self.head + i) % self.capacity;
            s.push(self.data[idx]);
        }
        s
    }

    /// Checks if the buffer ends with the given keyword suffix.
    pub fn ends_with(&mut self, keyword: &str) -> bool {
        self.check_timeout_and_clear();
        let kw_chars: Vec<char> = keyword.chars().collect();
        if kw_chars.is_empty() || kw_chars.len() > self.len {
            return false;
        }

        let start_offset = self.len - kw_chars.len();
        for (i, &kw_ch) in kw_chars.iter().enumerate() {
            let idx = (self.head + start_offset + i) % self.capacity;
            if self.data[idx] != kw_ch {
                return false;
            }
        }
        true
    }

    /// Securely zeroes out all memory slots and resets pointers.
    pub fn clear(&mut self) {
        self.data.zeroize();
        self.head = 0;
        self.len = 0;
        self.last_activity = Instant::now();
    }

    /// Returns current number of characters stored in buffer.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns maximum capacity of the buffer.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns duration since last keystroke activity.
    pub fn elapsed_since_activity(&self) -> Duration {
        self.last_activity.elapsed()
    }
}

impl Default for SecureRingBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SecureRingBuffer {
    fn drop(&mut self) {
        self.data.zeroize();
        self.unlock_memory();
    }
}

impl ZeroizeOnDrop for SecureRingBuffer {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_push_and_as_string() {
        let mut buf = SecureRingBuffer::with_capacity_and_timeout(5, Duration::from_secs(10));
        assert!(buf.is_empty());

        buf.push('a');
        buf.push('b');
        buf.push('c');
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.as_string(), "abc");

        buf.push('d');
        buf.push('e');
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.as_string(), "abcde");

        // Overflow: should drop 'a'
        buf.push('f');
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.as_string(), "bcdef");
    }

    #[test]
    fn test_pop_back_and_backspace() {
        let mut buf = SecureRingBuffer::with_capacity_and_timeout(5, Duration::from_secs(10));
        buf.push('h');
        buf.push('e');
        buf.push('y');
        assert_eq!(buf.as_string(), "hey");

        assert_eq!(buf.pop_back(), Some('y'));
        assert_eq!(buf.as_string(), "he");

        buf.push('l');
        buf.push('l');
        buf.push('o');
        assert_eq!(buf.as_string(), "hello");
    }

    #[test]
    fn test_ends_with_keyword() {
        let mut buf = SecureRingBuffer::with_capacity_and_timeout(10, Duration::from_secs(10));
        for ch in "hello :test".chars() {
            buf.push(ch);
        }
        assert!(buf.ends_with(":test"));
        assert!(!buf.ends_with(":tes"));
        assert!(!buf.ends_with(":other"));
    }

    #[test]
    fn test_timeout_clear() {
        let mut buf = SecureRingBuffer::with_capacity_and_timeout(10, Duration::from_millis(50));
        buf.push('x');
        buf.push('y');
        assert_eq!(buf.len(), 2);

        sleep(Duration::from_millis(70));
        assert!(buf.check_timeout_and_clear());
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.as_string(), "");
    }

    #[test]
    fn test_zeroize_on_clear() {
        let mut buf = SecureRingBuffer::with_capacity_and_timeout(5, Duration::from_secs(10));
        buf.push('a');
        buf.push('b');
        buf.clear();
        assert_eq!(buf.len(), 0);
        for &ch in &buf.data {
            assert_eq!(ch, '\0');
        }
    }
}
