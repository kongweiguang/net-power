-- @author kongweiguang
-- 常用查询索引，覆盖服务列表、规则加载和日志筛选。

CREATE INDEX IF NOT EXISTS idx_services_kind ON services(kind);
CREATE INDEX IF NOT EXISTS idx_services_deleted_at ON services(deleted_at);
CREATE INDEX IF NOT EXISTS idx_http_header_rules_service_id ON http_header_rules(service_id);
CREATE INDEX IF NOT EXISTS idx_body_rewrite_rules_service_id ON body_rewrite_rules(service_id);
CREATE INDEX IF NOT EXISTS idx_service_events_service_id_created_at ON service_events(service_id, created_at);
CREATE INDEX IF NOT EXISTS idx_connection_events_service_id_created_at ON connection_events(service_id, created_at);
