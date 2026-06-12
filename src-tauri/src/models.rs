//! @author kongweiguang
//! 前后端共享的数据模型。所有公开字段保持稳定的 camelCase JSON 协议。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 服务类型，决定运行时创建哪一种代理服务。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceKind {
    /// 固定上游的 HTTP/HTTPS 反向代理。
    HttpReverse,
    /// 通用 HTTP forward proxy，支持 CONNECT。
    HttpForward,
    /// TCP 四层转发。
    TcpForward,
    /// UDP 四层转发。
    UdpForward,
    /// SSH 本地端口转发。
    SshLocal,
    /// SSH 远程端口转发。
    SshRemote,
    /// SSH SOCKS5 动态代理。
    SshSocks,
}

impl ServiceKind {
    /// 返回数据库中持久化使用的 snake_case 值。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HttpReverse => "http_reverse",
            Self::HttpForward => "http_forward",
            Self::TcpForward => "tcp_forward",
            Self::UdpForward => "udp_forward",
            Self::SshLocal => "ssh_local",
            Self::SshRemote => "ssh_remote",
            Self::SshSocks => "ssh_socks",
        }
    }
}

impl TryFrom<&str> for ServiceKind {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "http_reverse" => Ok(Self::HttpReverse),
            "http_forward" => Ok(Self::HttpForward),
            "tcp_forward" => Ok(Self::TcpForward),
            "udp_forward" => Ok(Self::UdpForward),
            "ssh_local" => Ok(Self::SshLocal),
            "ssh_remote" => Ok(Self::SshRemote),
            "ssh_socks" => Ok(Self::SshSocks),
            _ => Err(format!("未知服务类型: {value}")),
        }
    }
}

impl fmt::Display for ServiceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 运行态状态由内存中的 ServiceManager 提供，数据库只保存配置。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeStatus {
    /// 未运行。
    #[default]
    Stopped,
    /// 正在启动。
    Starting,
    /// 正在运行。
    Running,
    /// 正在停止。
    Stopping,
    /// 运行失败，message 是可展示给用户的摘要。
    Failed { message: String },
}

/// 工具 HTTP 服务接口响应内容来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolServiceContentSource {
    /// 使用前端表单内填写的响应体。
    Inline,
    /// 从本地文件读取响应体。
    File,
}

impl ToolServiceContentSource {
    /// 返回数据库中持久化使用的 snake_case 值。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::File => "file",
        }
    }
}

impl TryFrom<&str> for ToolServiceContentSource {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "inline" => Ok(Self::Inline),
            "file" => Ok(Self::File),
            _ => Err(format!("未知工具服务响应来源: {value}")),
        }
    }
}

/// 工具 HTTP 服务接口路由输入。创建配置时会写入 SQLite，启动时进入运行态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolServiceRouteInput {
    /// HTTP 方法，支持 GET、POST、PUT、PATCH、DELETE、HEAD、OPTIONS 或 ANY。
    pub method: String,
    /// 精确匹配的请求路径，不包含 query。
    pub path: String,
    /// 响应状态码。
    pub response_status: u16,
    /// 响应 Content-Type。
    pub content_type: String,
    /// 响应体来源。
    pub content_source: ToolServiceContentSource,
    /// 手写响应体，仅 content_source 为 inline 时使用。
    pub body: Option<String>,
    /// 响应文件路径，仅 content_source 为 file 时使用。
    pub file_path: Option<String>,
}

/// 本地工具 HTTP 服务配置输入。创建后会写入 SQLite，启动和暂停只影响运行态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolServiceInput {
    /// 用户可读服务名称。
    pub name: String,
    /// 监听主机。
    pub host: String,
    /// 监听端口。
    pub port: u16,
    /// 静态目录路径；为空表示不挂载静态文件。
    pub static_root_dir: Option<String>,
    /// 静态目录挂载路径前缀。
    pub static_path_prefix: String,
    /// 接口路由列表，按数组顺序匹配。
    #[serde(default)]
    pub routes: Vec<ToolServiceRouteInput>,
}

/// 已持久化的本地工具 HTTP 服务配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolServiceConfig {
    /// 服务主键。
    pub id: String,
    /// 用户可读服务名称。
    pub name: String,
    /// 监听主机。
    pub host: String,
    /// 监听端口。
    pub port: u16,
    /// 静态目录路径；为空表示不挂载静态文件。
    pub static_root_dir: Option<String>,
    /// 静态目录挂载路径前缀。
    pub static_path_prefix: String,
    /// 接口路由列表，按数组顺序匹配。
    pub routes: Vec<ToolServiceRouteInput>,
    /// 创建时间，SQLite datetime 字符串。
    pub created_at: String,
    /// 更新时间，SQLite datetime 字符串。
    pub updated_at: String,
}

impl ToolServiceConfig {
    /// 转成运行态启动所需的输入结构。
    pub fn to_input(&self) -> ToolServiceInput {
        ToolServiceInput {
            name: self.name.clone(),
            host: self.host.clone(),
            port: self.port,
            static_root_dir: self.static_root_dir.clone(),
            static_path_prefix: self.static_path_prefix.clone(),
            routes: self.routes.clone(),
        }
    }

    /// 第一个可访问路径，用于展示访问 URL。
    pub fn first_access_path(&self) -> &str {
        self.static_root_dir
            .as_ref()
            .map(|_| self.static_path_prefix.as_str())
            .or_else(|| self.routes.first().map(|route| route.path.as_str()))
            .unwrap_or("/")
    }
}

/// 本地工具 HTTP 服务列表摘要。配置来自 SQLite，运行态来自内存管理器。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolServiceSummary {
    /// 服务主键。
    pub id: String,
    /// 用户可读服务名称。
    pub name: String,
    /// 监听主机。
    pub host: String,
    /// 监听端口。
    pub port: u16,
    /// 可直接访问的本地 URL。
    pub url: String,
    /// 静态目录路径，未挂载时为空。
    pub static_root_dir: Option<String>,
    /// 静态目录挂载路径前缀。
    pub static_path_prefix: String,
    /// 已配置接口路由数量。
    pub route_count: usize,
    /// 启动时间；已暂停服务为空。
    pub started_at: Option<String>,
    /// 累计请求数。
    pub total_requests: u64,
    /// 运行态。
    pub runtime_status: RuntimeStatus,
}

/// 服务列表行，前端 Dashboard 和各类型页面共用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSummary {
    /// 服务主键。
    pub id: String,
    /// 用户可读名称。
    pub name: String,
    /// 服务类型。
    pub kind: ServiceKind,
    /// 是否启用配置。
    pub enabled: bool,
    /// 应用启动时是否自动启动。
    pub auto_start: bool,
    /// 监听主机。
    pub listen_host: String,
    /// 监听端口。
    pub listen_port: u16,
    /// 目标地址摘要。
    pub target_label: String,
    /// 当前运行态。
    pub runtime_status: RuntimeStatus,
    /// 当前活跃连接数。
    pub active_connections: u64,
    /// 累计连接数。
    pub total_connections: u64,
}

/// 完整服务配置，编辑面板和服务启动都读取这个结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDetail {
    /// 服务主键。
    pub id: String,
    /// 用户可读名称。
    pub name: String,
    /// 服务类型。
    pub kind: ServiceKind,
    /// 是否启用配置。
    pub enabled: bool,
    /// 应用启动时是否自动启动。
    pub auto_start: bool,
    /// 监听主机。
    pub listen_host: String,
    /// 监听端口。
    pub listen_port: u16,
    /// 备注。
    pub notes: String,
    /// 创建时间，SQLite datetime 字符串。
    pub created_at: String,
    /// 更新时间，SQLite datetime 字符串。
    pub updated_at: String,
    /// HTTP 反向代理配置。
    pub http_reverse: Option<HttpReverseConfig>,
    /// HTTP forward proxy 配置。
    pub http_forward: Option<HttpForwardConfig>,
    /// TCP 转发配置。
    pub tcp_forward: Option<TcpForwardConfig>,
    /// UDP 转发配置。
    pub udp_forward: Option<UdpForwardConfig>,
    /// SSH 隧道配置。
    pub ssh_tunnel: Option<SshTunnelConfig>,
    /// Header 改写规则。
    pub header_rules: Vec<HeaderRule>,
    /// Body 改写规则。
    pub body_rewrite_rules: Vec<BodyRewriteRule>,
}

/// HTTP 反向代理配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpReverseConfig {
    /// 上游基础 URL。
    pub target_url: String,
    /// 是否保留原始 Host。
    pub preserve_host: bool,
    /// 请求超时时间，毫秒。
    pub request_timeout_ms: u64,
    /// 最大可改写 body 字节数。
    pub max_rewrite_body_bytes: u64,
    /// 遇到 Content-Encoding 时是否跳过 body 改写。
    pub skip_compressed_body: bool,
}

/// HTTP forward proxy 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpForwardConfig {
    /// 是否允许普通 HTTP 请求。
    pub allow_http: bool,
    /// 是否允许 CONNECT 隧道。
    pub allow_connect: bool,
    /// 建连超时时间，毫秒。
    pub connect_timeout_ms: u64,
    /// 空闲超时时间，毫秒。
    pub idle_timeout_ms: u64,
}

/// TCP 转发配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TcpForwardConfig {
    /// 目标主机。
    pub target_host: String,
    /// 目标端口。
    pub target_port: u16,
    /// 建连超时时间，毫秒。
    pub connect_timeout_ms: u64,
    /// 空闲超时时间，毫秒，0 表示不单独限制。
    pub idle_timeout_ms: u64,
}

/// UDP 转发配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UdpForwardConfig {
    /// 目标主机。
    pub target_host: String,
    /// 目标端口。
    pub target_port: u16,
    /// 空闲超时时间，毫秒。
    pub idle_timeout_ms: u64,
}

/// SSH 隧道配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshTunnelConfig {
    /// SSH profile 主键。
    pub ssh_profile_id: String,
    /// 隧道类型，当前支持 local、remote 和 socks。
    pub tunnel_type: String,
    /// 远端目标主机。
    pub target_host: Option<String>,
    /// 远端目标端口。
    pub target_port: Option<u16>,
    /// 远程绑定主机，ssh_remote 使用。
    pub remote_bind_host: Option<String>,
    /// 远程绑定端口，ssh_remote 使用。
    pub remote_bind_port: Option<u16>,
}

/// Header 改写规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderRule {
    /// 规则主键。
    pub id: String,
    /// request 或 response。
    pub phase: String,
    /// set 或 remove。
    pub action: String,
    /// Header 名称。
    pub name: String,
    /// set 操作使用的值。
    pub value: Option<String>,
    /// 是否启用。
    pub enabled: bool,
    /// 排序，数值越小越先执行。
    pub sort_order: i64,
}

/// Body 改写规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyRewriteRule {
    /// 规则主键。
    pub id: String,
    /// auto/json/form。
    pub body_type: String,
    /// 点号路径或表单 key。
    pub path: String,
    /// JSON 表达的目标值。
    pub value_json: String,
    /// 是否启用。
    pub enabled: bool,
    /// 排序，数值越小越先执行。
    pub sort_order: i64,
}

/// 创建或完整更新服务的输入。更新时会替换类型专用配置和规则集合。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateServiceInput {
    /// 用户可读名称。
    pub name: String,
    /// 服务类型。
    pub kind: ServiceKind,
    /// 是否启用配置。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 应用启动时是否自动启动。
    #[serde(default)]
    pub auto_start: bool,
    /// 监听主机。
    #[serde(default = "default_listen_host")]
    pub listen_host: String,
    /// 监听端口。
    pub listen_port: u16,
    /// 备注。
    #[serde(default)]
    pub notes: String,
    /// HTTP 反向代理配置。
    pub http_reverse: Option<HttpReverseConfig>,
    /// HTTP forward proxy 配置。
    pub http_forward: Option<HttpForwardConfig>,
    /// TCP 转发配置。
    pub tcp_forward: Option<TcpForwardConfig>,
    /// UDP 转发配置。
    pub udp_forward: Option<UdpForwardConfig>,
    /// SSH 隧道配置。
    pub ssh_tunnel: Option<SshTunnelConfig>,
    /// Header 改写规则。
    #[serde(default)]
    pub header_rules: Vec<HeaderRuleInput>,
    /// Body 改写规则。
    #[serde(default)]
    pub body_rewrite_rules: Vec<BodyRewriteRuleInput>,
}

/// Header 规则输入，不要求前端提供主键。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderRuleInput {
    /// request 或 response。
    pub phase: String,
    /// set 或 remove。
    pub action: String,
    /// Header 名称。
    pub name: String,
    /// set 操作使用的值。
    pub value: Option<String>,
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 排序，数值越小越先执行。
    #[serde(default)]
    pub sort_order: i64,
}

/// Body 改写规则输入，不要求前端提供主键。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyRewriteRuleInput {
    /// auto/json/form。
    pub body_type: String,
    /// 点号路径或表单 key。
    pub path: String,
    /// JSON 表达的目标值。
    pub value_json: String,
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 排序，数值越小越先执行。
    #[serde(default)]
    pub sort_order: i64,
}

/// 服务运行态列表行。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceRuntimeSummary {
    /// 服务主键。
    pub service_id: String,
    /// 服务类型。
    pub kind: ServiceKind,
    /// 监听地址。
    pub listen_addr: String,
    /// 运行状态。
    pub runtime_status: RuntimeStatus,
    /// 启动时间。
    pub started_at: Option<String>,
    /// 当前活跃连接数。
    pub active_connections: u64,
    /// 累计连接数。
    pub total_connections: u64,
    /// 累计入口字节数。
    pub bytes_in: u64,
    /// 累计出口字节数。
    pub bytes_out: u64,
}

/// 操作测试结果，用于服务配置和 SSH profile 连通性测试。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    /// 是否成功。
    pub ok: bool,
    /// 用户可见消息。
    pub message: String,
    /// 耗时，毫秒。
    pub duration_ms: u64,
}

/// SSH profile 认证类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshAuthType {
    /// 密码认证。
    Password,
    /// 私钥认证。
    PrivateKey,
    /// SSH agent 认证。
    Agent,
}

impl SshAuthType {
    /// 返回数据库中持久化使用的 snake_case 值。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::PrivateKey => "private_key",
            Self::Agent => "agent",
        }
    }
}

impl TryFrom<&str> for SshAuthType {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "password" => Ok(Self::Password),
            "private_key" => Ok(Self::PrivateKey),
            "agent" => Ok(Self::Agent),
            _ => Err(format!("未知 SSH 认证类型: {value}")),
        }
    }
}

/// SSH profile 列表/详情输出，不返回明文 secret。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshProfile {
    /// Profile 主键。
    pub id: String,
    /// 用户可读名称。
    pub name: String,
    /// SSH 主机。
    pub host: String,
    /// SSH 端口。
    pub port: u16,
    /// 登录用户名。
    pub username: String,
    /// 认证方式。
    pub auth_type: SshAuthType,
    /// 是否已保存密码 secret。
    pub has_password: bool,
    /// 私钥路径。
    pub private_key_path: Option<String>,
    /// 是否已保存 passphrase secret。
    pub has_passphrase: bool,
    /// known_hosts 策略。
    pub known_hosts_mode: String,
    /// known_hosts 文件路径。
    pub known_hosts_path: Option<String>,
    /// 连接超时时间，毫秒。
    pub connect_timeout_ms: u64,
    /// keepalive 间隔，毫秒。
    pub keepalive_interval_ms: u64,
    /// 可选跳板 SSH profile 主键；为空表示直连目标主机。
    pub jump_profile_id: Option<String>,
}

/// Rust 运行时使用的 SSH profile 详情，包含 secret id，但不会序列化返回前端。
#[derive(Debug, Clone)]
pub struct SshProfileRuntimeConfig {
    /// Profile 主键。
    pub id: String,
    /// 用户可读名称。
    pub name: String,
    /// SSH 主机。
    pub host: String,
    /// SSH 端口。
    pub port: u16,
    /// 登录用户名。
    pub username: String,
    /// 认证方式。
    pub auth_type: SshAuthType,
    /// 密码 secret 主键。
    pub password_secret_id: Option<String>,
    /// 私钥路径。
    pub private_key_path: Option<String>,
    /// 私钥 passphrase secret 主键。
    pub private_key_passphrase_secret_id: Option<String>,
    /// known_hosts 策略。
    pub known_hosts_mode: String,
    /// known_hosts 文件路径。
    pub known_hosts_path: Option<String>,
    /// 连接超时时间，毫秒。
    pub connect_timeout_ms: u64,
    /// keepalive 间隔，毫秒。
    pub keepalive_interval_ms: u64,
    /// 可选跳板 SSH profile 主键；运行时按该字段解析多级跳板链。
    pub jump_profile_id: Option<String>,
}

/// 创建或更新 SSH profile 的输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshProfileInput {
    /// 用户可读名称。
    pub name: String,
    /// SSH 主机。
    pub host: String,
    /// SSH 端口。
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    /// 登录用户名。
    pub username: String,
    /// 认证方式。
    pub auth_type: SshAuthType,
    /// 新密码。为空时更新保留旧 secret。
    pub password: Option<String>,
    /// 私钥路径。
    pub private_key_path: Option<String>,
    /// 新私钥 passphrase。为空时更新保留旧 secret。
    pub private_key_passphrase: Option<String>,
    /// known_hosts 策略。
    #[serde(default = "default_known_hosts_mode")]
    pub known_hosts_mode: String,
    /// known_hosts 文件路径。
    pub known_hosts_path: Option<String>,
    /// 连接超时时间，毫秒。
    #[serde(default = "default_connect_timeout_ms")]
    pub connect_timeout_ms: u64,
    /// keepalive 间隔，毫秒。
    #[serde(default = "default_keepalive_interval_ms")]
    pub keepalive_interval_ms: u64,
    /// 可选跳板 SSH profile 主键；为空表示直连，引用链可表达多级跳板。
    pub jump_profile_id: Option<String>,
}

/// 应用设置键值。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSetting {
    /// 设置 key。
    pub key: String,
    /// JSON 字符串形式的设置值。
    pub value_json: String,
}

/// 系统登录时自动启动应用的状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AutostartStatus {
    /// 当前是否已注册系统开机启动项。
    pub enabled: bool,
    /// 当前平台或运行环境是否支持开机启动。
    pub supported: bool,
    /// 面向用户展示的状态说明。
    pub message: String,
}

/// 日志筛选条件。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFilter {
    /// 服务主键，可为空。
    pub service_id: Option<String>,
    /// 日志级别，可为空。
    pub level: Option<String>,
    /// 协议筛选，预留给连接日志。
    pub protocol: Option<String>,
    /// 关键词。
    pub keyword: Option<String>,
    /// 起始时间，使用 SQLite 可解析的本地时间或 ISO 字符串。
    pub created_after: Option<String>,
    /// 结束时间，使用 SQLite 可解析的本地时间或 ISO 字符串。
    pub created_before: Option<String>,
    /// 最大返回行数。
    pub limit: Option<u32>,
}

/// 日志列表行。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRow {
    /// 日志主键。
    pub id: i64,
    /// 服务主键。
    pub service_id: Option<String>,
    /// 日志级别。
    pub level: String,
    /// 消息。
    pub message: String,
    /// 结构化元数据 JSON 字符串。
    pub meta_json: String,
    /// 创建时间。
    pub created_at: String,
}

/// 系统代理状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProxyStatus {
    /// 是否已开启。
    pub enabled: bool,
    /// 代理主机。
    pub proxy_host: String,
    /// 代理端口。
    pub proxy_port: Option<u16>,
    /// 绕过列表。
    pub bypass: String,
    /// 状态说明。
    pub message: String,
}

/// 可用于设置系统代理的目标。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProxyTarget {
    /// 代理主机。
    pub proxy_host: String,
    /// 代理端口。
    pub proxy_port: u16,
    /// 绕过列表。
    pub bypass: String,
}

/// 系统代理配置档。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProxyProfile {
    /// 配置档主键。
    pub id: String,
    /// 用户可读名称。
    pub name: String,
    /// 代理主机。
    pub proxy_host: String,
    /// 代理端口。
    pub proxy_port: u16,
    /// 绕过列表。
    pub bypass: String,
    /// 是否为最近一次通过配置档启用的目标。
    pub active: bool,
    /// 创建时间。
    pub created_at: String,
    /// 更新时间。
    pub updated_at: String,
}

/// 创建或更新系统代理配置档的输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProxyProfileInput {
    /// 用户可读名称。
    pub name: String,
    /// 代理主机。
    pub proxy_host: String,
    /// 代理端口。
    pub proxy_port: u16,
    /// 绕过列表。
    pub bypass: String,
}

/// 系统信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    /// 操作系统。
    pub os: String,
    /// CPU 架构。
    pub arch: String,
    /// 应用版本。
    pub app_version: String,
    /// 应用数据目录。
    pub data_dir: String,
}

/// 服务事件推送 payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEventPayload {
    /// 服务主键。
    pub service_id: Option<String>,
    /// 事件级别。
    pub level: String,
    /// 用户可见消息。
    pub message: String,
}

/// 连接事件推送 payload。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionEventPayload {
    /// 服务主键。
    pub service_id: String,
    /// 协议名称。
    pub protocol: String,
    /// 事件类型。
    pub event_type: String,
    /// 目标地址。
    pub target_addr: String,
}

fn default_true() -> bool {
    true
}

fn default_listen_host() -> String {
    "127.0.0.1".to_string()
}

fn default_ssh_port() -> u16 {
    22
}

fn default_known_hosts_mode() -> String {
    "accept_new".to_string()
}

fn default_connect_timeout_ms() -> u64 {
    10_000
}

fn default_keepalive_interval_ms() -> u64 {
    30_000
}
