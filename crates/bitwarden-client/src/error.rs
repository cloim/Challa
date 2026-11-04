use thiserror::Error;

#[derive(Error, Debug)]
pub enum BitwardenError {
    #[error("보관함이 잠겨 있습니다")]
    VaultLocked,

    #[error("해당 아이템을 찾을 수 없습니다: {0}")]
    ItemNotFound(String),

    #[error("해당 필드를 찾을 수 없습니다")]
    FieldNotFound,

    #[error("인증 실패: {0}")]
    AuthenticationFailed(String),

    #[error("네트워크 오류: {0}")]
    Network(String),

    #[error("내부 오류: {0}")]
    Internal(String),
}
