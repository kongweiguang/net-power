-- @author kongweiguang
-- Net Power 初始数据库结构。SQLite 是代理服务配置的唯一持久化来源。

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS app_settings (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS services (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (
    kind IN (
      'http_reverse',
      'http_forward',
      'tcp_forward',
      'udp_forward',
      'ssh_local',
      'ssh_remote',
      'ssh_socks'
    )
  ),
  enabled INTEGER NOT NULL DEFAULT 1,
  auto_start INTEGER NOT NULL DEFAULT 0,
  listen_host TEXT NOT NULL DEFAULT '127.0.0.1',
  listen_port INTEGER NOT NULL,
  notes TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  deleted_at TEXT
);

CREATE TABLE IF NOT EXISTS http_reverse_configs (
  service_id TEXT PRIMARY KEY,
  target_url TEXT NOT NULL,
  preserve_host INTEGER NOT NULL DEFAULT 0,
  request_timeout_ms INTEGER NOT NULL DEFAULT 30000,
  max_rewrite_body_bytes INTEGER NOT NULL DEFAULT 10485760,
  skip_compressed_body INTEGER NOT NULL DEFAULT 1,
  FOREIGN KEY (service_id) REFERENCES services(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS http_forward_configs (
  service_id TEXT PRIMARY KEY,
  allow_http INTEGER NOT NULL DEFAULT 1,
  allow_connect INTEGER NOT NULL DEFAULT 1,
  connect_timeout_ms INTEGER NOT NULL DEFAULT 10000,
  idle_timeout_ms INTEGER NOT NULL DEFAULT 60000,
  FOREIGN KEY (service_id) REFERENCES services(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS tcp_forward_configs (
  service_id TEXT PRIMARY KEY,
  target_host TEXT NOT NULL,
  target_port INTEGER NOT NULL,
  connect_timeout_ms INTEGER NOT NULL DEFAULT 10000,
  idle_timeout_ms INTEGER NOT NULL DEFAULT 0,
  FOREIGN KEY (service_id) REFERENCES services(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS udp_forward_configs (
  service_id TEXT PRIMARY KEY,
  target_host TEXT NOT NULL,
  target_port INTEGER NOT NULL,
  idle_timeout_ms INTEGER NOT NULL DEFAULT 60000,
  FOREIGN KEY (service_id) REFERENCES services(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ssh_profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  host TEXT NOT NULL,
  port INTEGER NOT NULL DEFAULT 22,
  username TEXT NOT NULL,
  auth_type TEXT NOT NULL CHECK (auth_type IN ('password', 'private_key', 'agent')),
  password_secret_id TEXT,
  private_key_path TEXT,
  private_key_passphrase_secret_id TEXT,
  known_hosts_mode TEXT NOT NULL DEFAULT 'accept_new' CHECK (
    known_hosts_mode IN ('strict', 'accept_new', 'insecure_skip')
  ),
  known_hosts_path TEXT,
  connect_timeout_ms INTEGER NOT NULL DEFAULT 10000,
  keepalive_interval_ms INTEGER NOT NULL DEFAULT 30000,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  deleted_at TEXT
);

CREATE TABLE IF NOT EXISTS ssh_tunnel_configs (
  service_id TEXT PRIMARY KEY,
  ssh_profile_id TEXT NOT NULL,
  tunnel_type TEXT NOT NULL CHECK (tunnel_type IN ('local', 'remote', 'socks')),
  target_host TEXT,
  target_port INTEGER,
  remote_bind_host TEXT,
  remote_bind_port INTEGER,
  FOREIGN KEY (service_id) REFERENCES services(id) ON DELETE CASCADE,
  FOREIGN KEY (ssh_profile_id) REFERENCES ssh_profiles(id)
);

CREATE TABLE IF NOT EXISTS http_header_rules (
  id TEXT PRIMARY KEY,
  service_id TEXT NOT NULL,
  phase TEXT NOT NULL DEFAULT 'request' CHECK (phase IN ('request', 'response')),
  action TEXT NOT NULL CHECK (action IN ('set', 'remove')),
  name TEXT NOT NULL,
  value TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (service_id) REFERENCES services(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS body_rewrite_rules (
  id TEXT PRIMARY KEY,
  service_id TEXT NOT NULL,
  body_type TEXT NOT NULL DEFAULT 'auto' CHECK (body_type IN ('auto', 'json', 'form')),
  path TEXT NOT NULL,
  value_json TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (service_id) REFERENCES services(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS system_proxy_profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  proxy_host TEXT NOT NULL DEFAULT '127.0.0.1',
  proxy_port INTEGER NOT NULL,
  bypass TEXT NOT NULL DEFAULT '',
  active INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS secrets (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (
    kind IN ('http_header', 'ssh_password', 'ssh_key_passphrase', 'generic')
  ),
  ciphertext BLOB NOT NULL,
  nonce BLOB NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS service_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  service_id TEXT,
  level TEXT NOT NULL CHECK (level IN ('trace', 'debug', 'info', 'warn', 'error')),
  message TEXT NOT NULL,
  meta_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (service_id) REFERENCES services(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS connection_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  service_id TEXT,
  protocol TEXT NOT NULL CHECK (protocol IN ('http', 'https', 'tcp', 'udp', 'ssh')),
  remote_addr TEXT NOT NULL DEFAULT '',
  target_addr TEXT NOT NULL DEFAULT '',
  event_type TEXT NOT NULL,
  method TEXT NOT NULL DEFAULT '',
  host TEXT NOT NULL DEFAULT '',
  path TEXT NOT NULL DEFAULT '',
  status_code INTEGER,
  bytes_in INTEGER NOT NULL DEFAULT 0,
  bytes_out INTEGER NOT NULL DEFAULT 0,
  duration_ms INTEGER NOT NULL DEFAULT 0,
  error TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (service_id) REFERENCES services(id) ON DELETE SET NULL
);
