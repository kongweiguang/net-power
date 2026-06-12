//! @author kongweiguang
//! 应用统一错误类型。Command 层会把这些错误转成用户可读字符串。

use thiserror::Error;

/// Net Power 后端统一错误。
#[derive(Debug, Error)]
pub enum AppError {
    /// 文件、网络监听和子进程等 IO 错误。
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    /// SQLite 数据库错误。
    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),
    /// JSON 序列化或反序列化错误。
    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),
    /// HTTP 客户端请求错误。
    #[error("HTTP 请求失败: {0}")]
    Http(#[from] reqwest::Error),
    /// URL 解析错误。
    #[error("URL 无效: {0}")]
    Url(#[from] url::ParseError),
    /// keyring 访问错误。
    #[error("系统钥匙串错误: {0}")]
    Keyring(#[from] keyring::Error),
    /// SSH 握手、认证或通道错误。
    #[error("SSH 错误: {0}")]
    Ssh(#[from] ssh2::Error),
    /// 加密或解密失败。
    #[error("敏感信息加密失败: {0}")]
    Crypto(String),
    /// 资源不存在。
    #[error("未找到: {0}")]
    NotFound(String),
    /// 用户输入或配置不合法。
    #[error("参数无效: {0}")]
    InvalidInput(String),
    /// 运行态冲突，例如重复启动或端口冲突。
    #[error("运行态冲突: {0}")]
    Conflict(String),
    /// 当前平台暂不支持的能力。
    #[error("当前平台暂不支持: {0}")]
    Unsupported(String),
    /// 其他需要透传的用户可见错误。
    #[error("{0}")]
    Message(String),
}

impl From<AppError> for String {
    fn from(value: AppError) -> Self {
        value.to_string()
    }
}

/// 后端通用 Result 类型。
pub type AppResult<T> = Result<T, AppError>;
