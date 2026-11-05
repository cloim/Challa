import type { VaultItem } from "./api";

export function vaultItemLabel(item: VaultItem): string {
  return `${item.name} (${item.username || "아이디 없음"})`;
}

const initials = (text: string) => [...text].map(char => {
  const code = char.charCodeAt(0) - 0xac00;
  return code >= 0 && code < 11172 ? "ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ"[Math.floor(code / 588)] : char;
}).join("");

export function searchVaultItems(items: VaultItem[], query: string): VaultItem[] {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return items.filter(item => {
    const text = `${item.name} ${item.username || ""}`.toLocaleLowerCase();
    const short = initials(text);
    return terms.every(term => text.includes(term) || (/^[ㄱ-ㅎ]+$/.test(term) && short.includes(term)));
  });
}
