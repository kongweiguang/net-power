-- @author kongweiguang
-- 为本地工具 HTTP 服务增加静态目录访问模式：目录浏览或静态网站。

ALTER TABLE tool_services
  ADD COLUMN static_mode TEXT NOT NULL DEFAULT 'directory'
  CHECK (static_mode IN ('directory', 'site'));
