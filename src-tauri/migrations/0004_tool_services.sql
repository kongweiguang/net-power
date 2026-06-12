-- @author kongweiguang
-- 持久化本地工具 HTTP 服务配置，暂停只影响运行态，不删除配置。

CREATE TABLE IF NOT EXISTS tool_services (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  host TEXT NOT NULL DEFAULT '127.0.0.1',
  port INTEGER NOT NULL,
  static_root_dir TEXT,
  static_path_prefix TEXT NOT NULL DEFAULT '/',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  deleted_at TEXT
);

CREATE TABLE IF NOT EXISTS tool_service_routes (
  id TEXT PRIMARY KEY,
  service_id TEXT NOT NULL,
  method TEXT NOT NULL CHECK (method IN ('GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS', 'ANY')),
  path TEXT NOT NULL,
  response_status INTEGER NOT NULL,
  content_type TEXT NOT NULL,
  content_source TEXT NOT NULL CHECK (content_source IN ('inline', 'file')),
  body TEXT,
  file_path TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  FOREIGN KEY (service_id) REFERENCES tool_services(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_tool_services_deleted_at ON tool_services(deleted_at);
CREATE INDEX IF NOT EXISTS idx_tool_service_routes_service_id ON tool_service_routes(service_id);
