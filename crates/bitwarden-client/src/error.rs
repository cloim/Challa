#[derive(Debug, thiserror::Error)]
pub enum BitwardenError {
    #[error("Bitwarden 보관함을 먼저 잠금 해제해 주세요")]
    VaultLocked,
    #[error("Bitwarden 항목을 찾을 수 없어요")]
    ItemNotFound(String),
    #[error("선택한 필드를 사용할 수 없어요. 항목을 확인하고 다시 동기화해 주세요")]
    FieldNotFound,
    #[error("Bitwarden CLI가 필요해요. 공식 CLI를 설치한 뒤 Challa를 다시 실행해 주세요")]
    CliUnavailable,
    #[error("Bitwarden 명령이 실패했어요. 인증 정보, 서버와 네트워크를 확인해 주세요")]
    CommandFailed,
    #[error("서버 주소를 CLI에 설정하지 못했어요. 서버 주소와 로그인 상태를 확인해 주세요")]
    ServerConfiguration,
    #[error("이 CLI 프로필은 이미 로그인되어 있어요. 보관함을 새로고침하거나 로그아웃 후 다시 연결해 주세요")]
    AlreadyAuthenticated,
    #[error("Vaultwarden이 개인 API 키 인증을 거절했어요. 해당 서버에서 발급한 Client ID와 Client Secret인지 확인해 주세요")]
    AuthenticationRejected,
    #[error("CLI 로그인 단계가 실패했어요. 서버의 로그인 정책과 CLI 호환성을 확인해 주세요")]
    AuthenticationFailed,
    #[error("개인 API 키의 Client ID는 user.로 시작해야 해요. 관리자 토큰이나 조직 API 키 대신 개인 API 키를 입력해 주세요")]
    PersonalApiKeyRequired,
    #[error(
        "CLI가 서버 인증서를 신뢰하지 못했어요. 인증서의 도메인과 전체 인증서 체인을 확인해 주세요"
    )]
    Tls,
    #[error("CLI가 서버에 연결하지 못했어요. 서버 주소, 프록시와 네트워크 연결을 확인해 주세요")]
    Connection,
    #[error("Bitwarden 응답 시간이 초과됐어요. 다시 시도해 주세요")]
    Timeout,
    #[error("Bitwarden 응답 형식을 확인할 수 없어요")]
    InvalidResponse,
    #[error("이 항목은 마스터 비밀번호 재확인이 필요해요. Bitwarden 앱에서 사용해 주세요")]
    RepromptRequired,
    #[error("HTTPS 서버 주소를 입력해 주세요")]
    InvalidServer,
    #[error("Bitwarden 작업이 진행 중이에요. 잠시 후 다시 시도해 주세요")]
    Busy,
    #[error(
        "Bitwarden 암호화 프로필을 사용할 수 없어요. Windows 계정과 저장 공간을 확인해 주세요"
    )]
    ProfileStorage,
}
