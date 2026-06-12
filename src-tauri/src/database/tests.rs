// @author kongweiguang
// Database 单元测试和外置测试模块声明。

#[cfg(test)]
#[path = "../database_remote_tests.rs"]
mod remote_tests;

#[cfg(test)]
#[path = "../database_ssh_jump_tests.rs"]
mod ssh_jump_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_http_service() -> CreateServiceInput {
        CreateServiceInput {
            name: "测试反向代理".to_string(),
            kind: ServiceKind::HttpReverse,
            enabled: true,
            auto_start: false,
            listen_host: "127.0.0.1".to_string(),
            listen_port: 18080,
            notes: String::new(),
            http_reverse: Some(HttpReverseConfig {
                target_url: "http://127.0.0.1:19090".to_string(),
                preserve_host: false,
                request_timeout_ms: 30_000,
                max_rewrite_body_bytes: 10_485_760,
                skip_compressed_body: true,
            }),
            http_forward: None,
            tcp_forward: None,
            udp_forward: None,
            ssh_tunnel: None,
            header_rules: vec![HeaderRuleInput {
                phase: "request".to_string(),
                action: "set".to_string(),
                name: "x-test".to_string(),
                value: Some("ok".to_string()),
                enabled: true,
                sort_order: 0,
            }],
            body_rewrite_rules: vec![BodyRewriteRuleInput {
                body_type: "json".to_string(),
                path: "user.name".to_string(),
                value_json: "\"kong\"".to_string(),
                enabled: true,
                sort_order: 0,
            }],
        }
    }

    fn sample_ssh_profile() -> SshProfileInput {
        SshProfileInput {
            name: "测试 SSH".to_string(),
            host: "127.0.0.1".to_string(),
            port: 22,
            username: "tester".to_string(),
            auth_type: SshAuthType::Agent,
            password: None,
            private_key_path: None,
            private_key_passphrase: None,
            known_hosts_mode: "insecure_skip".to_string(),
            known_hosts_path: None,
            connect_timeout_ms: 10_000,
            keepalive_interval_ms: 30_000,
            jump_profile_id: None,
        }
    }

    #[test]
    fn migration_creates_default_settings() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let settings = db.list_settings().expect("默认设置应可读取");
        assert!(settings.iter().any(|row| row.key == "app.initialized"));
    }

    #[test]
    fn service_crud_roundtrip_keeps_rules() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let created = db
            .create_service(&sample_http_service())
            .expect("服务应创建成功");
        assert_eq!(created.header_rules.len(), 1);
        assert_eq!(created.body_rewrite_rules.len(), 1);

        let listed = db.list_service_summaries().expect("服务列表应可读取");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].target_label, "http://127.0.0.1:19090");

        db.delete_service(&created.id).expect("服务应可软删除");
        assert!(db.list_services().expect("服务列表应可读取").is_empty());
    }

    #[test]
    fn ssh_local_service_roundtrip_keeps_profile_reference() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let profile = db
            .create_ssh_profile(&sample_ssh_profile(), None, None)
            .expect("SSH Profile 应创建成功");
        let service = db
            .create_service(&CreateServiceInput {
                name: "测试 SSH 隧道".to_string(),
                kind: ServiceKind::SshLocal,
                enabled: true,
                auto_start: false,
                listen_host: "127.0.0.1".to_string(),
                listen_port: 19022,
                notes: String::new(),
                http_reverse: None,
                http_forward: None,
                tcp_forward: None,
                udp_forward: None,
                ssh_tunnel: Some(SshTunnelConfig {
                    ssh_profile_id: profile.id.clone(),
                    tunnel_type: "local".to_string(),
                    target_host: Some("127.0.0.1".to_string()),
                    target_port: Some(5432),
                    remote_bind_host: None,
                    remote_bind_port: None,
                }),
                header_rules: Vec::new(),
                body_rewrite_rules: Vec::new(),
            })
            .expect("SSH local 服务应创建成功");

        let tunnel = service.ssh_tunnel.expect("SSH 隧道配置应存在");
        assert_eq!(tunnel.ssh_profile_id, profile.id);
        assert_eq!(tunnel.target_host.as_deref(), Some("127.0.0.1"));
        assert_eq!(tunnel.target_port, Some(5432));
    }

    #[test]
    fn ssh_socks_service_roundtrip_only_requires_profile() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let profile = db
            .create_ssh_profile(&sample_ssh_profile(), None, None)
            .expect("SSH Profile 应创建成功");
        let service = db
            .create_service(&CreateServiceInput {
                name: "测试 SSH SOCKS".to_string(),
                kind: ServiceKind::SshSocks,
                enabled: true,
                auto_start: false,
                listen_host: "127.0.0.1".to_string(),
                listen_port: 19080,
                notes: String::new(),
                http_reverse: None,
                http_forward: None,
                tcp_forward: None,
                udp_forward: None,
                ssh_tunnel: Some(SshTunnelConfig {
                    ssh_profile_id: profile.id.clone(),
                    tunnel_type: "socks".to_string(),
                    target_host: None,
                    target_port: None,
                    remote_bind_host: None,
                    remote_bind_port: None,
                }),
                header_rules: Vec::new(),
                body_rewrite_rules: Vec::new(),
            })
            .expect("SSH SOCKS 服务应创建成功");

        assert_eq!(service.kind, ServiceKind::SshSocks);
        assert_eq!(target_label(&service), "SOCKS5 动态代理");
        let tunnel = service.ssh_tunnel.expect("SSH SOCKS 配置应存在");
        assert_eq!(tunnel.ssh_profile_id, profile.id);
        assert_eq!(tunnel.tunnel_type, "socks");
        assert!(tunnel.target_host.is_none());
        assert!(tunnel.target_port.is_none());
    }

    #[test]
    fn password_ssh_profile_requires_saved_secret() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let mut input = sample_ssh_profile();
        input.auth_type = SshAuthType::Password;
        input.password = Some("secret".to_string());

        let failed = db.create_ssh_profile(&input, None, None);
        assert!(matches!(failed, Err(AppError::InvalidInput(_))));

        let created = db
            .create_ssh_profile(&input, Some("password-secret-id"), None)
            .expect("带 secret id 的密码配置应可创建");
        assert!(created.has_password);
    }

    #[test]
    fn system_proxy_profile_crud_and_target_resolution() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let created = db
            .create_system_proxy_profile(&SystemProxyProfileInput {
                name: " 办公网代理 ".to_string(),
                proxy_host: " 127.0.0.1 ".to_string(),
                proxy_port: 7890,
                bypass: " localhost;127.* ".to_string(),
            })
            .expect("系统代理配置档应可创建");
        assert_eq!(created.name, "办公网代理");
        assert_eq!(created.proxy_host, "127.0.0.1");
        assert_eq!(created.bypass, "localhost;127.*");
        assert!(!created.active);

        let updated = db
            .update_system_proxy_profile(
                &created.id,
                &SystemProxyProfileInput {
                    name: "家庭代理".to_string(),
                    proxy_host: "192.168.1.10".to_string(),
                    proxy_port: 1080,
                    bypass: "localhost;10.*".to_string(),
                },
            )
            .expect("系统代理配置档应可更新");
        assert_eq!(updated.proxy_host, "192.168.1.10");
        assert_eq!(updated.proxy_port, 1080);

        let target = db
            .get_system_proxy_target(&created.id)
            .expect("配置档应可解析为系统代理目标");
        assert_eq!(target.proxy_host, "192.168.1.10");
        assert_eq!(target.proxy_port, 1080);
        assert_eq!(target.bypass, "localhost;10.*");

        db.set_active_system_proxy_profile(Some(&created.id))
            .expect("配置档应可标记 active");
        let profiles = db
            .list_system_proxy_profiles()
            .expect("系统代理配置档列表应可读取");
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].active);

        db.delete_system_proxy_profile(&created.id)
            .expect("系统代理配置档应可删除");
        assert!(db
            .list_system_proxy_profiles()
            .expect("删除后配置档列表应可读取")
            .is_empty());
    }

    #[test]
    fn tool_service_config_roundtrip_is_persisted() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let created = db
            .create_tool_service(&ToolServiceInput {
                name: " 本地 HTTP ".to_string(),
                host: " 127.0.0.1 ".to_string(),
                port: 18081,
                static_root_dir: Some(" C:/site ".to_string()),
                static_path_prefix: " public/ ".to_string(),
                routes: vec![ToolServiceRouteInput {
                    method: "get".to_string(),
                    path: "api/ping".to_string(),
                    response_status: 201,
                    content_type: " application/json; charset=utf-8 ".to_string(),
                    content_source: ToolServiceContentSource::Inline,
                    body: Some("{\"ok\":true}".to_string()),
                    file_path: None,
                }],
            })
            .expect("工具服务配置应可创建");

        assert_eq!(created.name, "本地 HTTP");
        assert_eq!(created.host, "127.0.0.1");
        assert_eq!(created.static_path_prefix, "/public");
        assert_eq!(created.routes[0].method, "GET");
        assert_eq!(created.routes[0].path, "/api/ping");

        let listed = db
            .list_tool_service_summaries()
            .expect("工具服务列表应可读取");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].runtime_status, RuntimeStatus::Stopped);
        assert_eq!(listed[0].url, "http://127.0.0.1:18081/public");

        db.delete_tool_service(&created.id)
            .expect("工具服务配置应可软删除");
        assert!(db
            .list_tool_services()
            .expect("删除后工具服务列表应可读取")
            .is_empty());
    }

    #[test]
    fn service_update_rolls_back_when_detail_insert_fails() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let created = db
            .create_service(&sample_http_service())
            .expect("服务应创建成功");

        let failed = db.update_service(
            &created.id,
            &CreateServiceInput {
                name: "无效 SSH 隧道".to_string(),
                kind: ServiceKind::SshLocal,
                enabled: true,
                auto_start: false,
                listen_host: "127.0.0.1".to_string(),
                listen_port: 19022,
                notes: "should rollback".to_string(),
                http_reverse: None,
                http_forward: None,
                tcp_forward: None,
                udp_forward: None,
                ssh_tunnel: Some(SshTunnelConfig {
                    ssh_profile_id: "missing-profile".to_string(),
                    tunnel_type: "local".to_string(),
                    target_host: Some("127.0.0.1".to_string()),
                    target_port: Some(5432),
                    remote_bind_host: None,
                    remote_bind_port: None,
                }),
                header_rules: Vec::new(),
                body_rewrite_rules: Vec::new(),
            },
        );

        assert!(failed.is_err());
        let current = db.get_service(&created.id).expect("原服务应仍可读取");
        assert_eq!(current.name, "测试反向代理");
        assert_eq!(current.kind, ServiceKind::HttpReverse);
        assert!(current.http_reverse.is_some());
        assert_eq!(current.header_rules.len(), 1);
        assert_eq!(current.body_rewrite_rules.len(), 1);
    }

    #[test]
    fn list_logs_merges_service_and_connection_events_with_protocol_filter() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let service = db
            .create_service(&sample_http_service())
            .expect("服务应创建成功");
        db.insert_service_event(Some(&service.id), "info", "服务已启动", "{}")
            .expect("服务事件应写入成功");
        db.insert_connection_event(
            &service.id,
            "http",
            "127.0.0.1:50000",
            "127.0.0.1:19090",
            "request",
            "POST",
            "example.test",
            "/api",
            Some(200),
            12,
            34,
            56,
            "",
        )
        .expect("连接事件应写入成功");

        let all = db
            .list_logs(&LogFilter {
                service_id: Some(service.id.clone()),
                limit: Some(20),
                ..LogFilter::default()
            })
            .expect("日志应可查询");
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|row| row.message == "服务已启动"));
        assert!(all
            .iter()
            .any(|row| row.message.contains("http POST example.test/api -> 200")));

        let protocol_logs = db
            .list_logs(&LogFilter {
                service_id: Some(service.id.clone()),
                protocol: Some("http".to_string()),
                keyword: Some("example.test".to_string()),
                limit: Some(20),
                ..LogFilter::default()
            })
            .expect("协议日志应可查询");
        assert_eq!(protocol_logs.len(), 1);
        assert_eq!(protocol_logs[0].level, "info");
        assert!(protocol_logs[0].meta_json.contains("\"protocol\":\"http\""));

        db.clear_logs(Some(&service.id))
            .expect("日志应可按服务清理");
        assert!(db
            .list_logs(&LogFilter {
                service_id: Some(service.id),
                limit: Some(20),
                ..LogFilter::default()
            })
            .expect("清理后日志应可查询")
            .is_empty());
    }

    #[test]
    fn list_logs_time_range_filters_service_and_connection_events() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let service = db
            .create_service(&sample_http_service())
            .expect("服务应创建成功");
        insert_service_event_at(&db, &service.id, "窗口前服务日志", "2026-01-01 09:59:59");
        insert_service_event_at(&db, &service.id, "窗口内服务日志", "2026-01-01 10:10:00");
        insert_connection_event_at(
            &db,
            &service.id,
            "inside.example",
            "/inside",
            "2026-01-01 10:20:00",
        );
        insert_connection_event_at(
            &db,
            &service.id,
            "after.example",
            "/after",
            "2026-01-01 11:00:01",
        );

        let logs = db
            .list_logs(&LogFilter {
                service_id: Some(service.id),
                created_after: Some("2026-01-01 10:00:00".to_string()),
                created_before: Some("2026-01-01 10:59:59".to_string()),
                limit: Some(20),
                ..LogFilter::default()
            })
            .expect("时间范围日志应可查询");

        assert_eq!(logs.len(), 2);
        assert!(logs.iter().any(|row| row.message == "窗口内服务日志"));
        assert!(logs
            .iter()
            .any(|row| row.message.contains("inside.example")));
        assert!(!logs.iter().any(|row| row.message.contains("窗口前")));
        assert!(!logs.iter().any(|row| row.message.contains("after.example")));
    }

    #[test]
    fn prune_logs_applies_retention_and_total_row_limit() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let service = db
            .create_service(&sample_http_service())
            .expect("服务应创建成功");
        insert_service_event_at(&db, &service.id, "过期服务日志", "2000-01-01 00:00:00");
        insert_connection_event_at(
            &db,
            &service.id,
            "old.example",
            "/old",
            "2000-01-01 00:00:01",
        );
        insert_service_event_at(&db, &service.id, "新服务日志 1", "2999-01-01 00:00:01");
        insert_connection_event_at(
            &db,
            &service.id,
            "new-a.example",
            "/a",
            "2999-01-01 00:00:02",
        );
        insert_service_event_at(&db, &service.id, "新服务日志 2", "2999-01-01 00:00:03");
        insert_connection_event_at(
            &db,
            &service.id,
            "new-b.example",
            "/b",
            "2999-01-01 00:00:04",
        );

        db.prune_logs(7, 3).expect("日志保留清理应成功");

        let logs = db
            .list_logs(&LogFilter {
                service_id: Some(service.id),
                limit: Some(20),
                ..LogFilter::default()
            })
            .expect("日志应可查询");
        assert_eq!(logs.len(), 3);
        assert!(logs.iter().any(|row| row.message.contains("new-b.example")));
        assert!(logs.iter().any(|row| row.message == "新服务日志 2"));
        assert!(logs.iter().any(|row| row.message.contains("new-a.example")));
        assert!(!logs.iter().any(|row| row.message.contains("过期")));
        assert!(!logs.iter().any(|row| row.message.contains("old.example")));
    }

    fn insert_service_event_at(db: &Database, service_id: &str, message: &str, created_at: &str) {
        let conn = db.conn().expect("数据库连接应可用");
        conn.execute(
            "INSERT INTO service_events (service_id, level, message, meta_json, created_at)
             VALUES (?1, 'info', ?2, '{}', ?3)",
            params![service_id, message, created_at],
        )
        .expect("服务日志应写入成功");
    }

    fn insert_connection_event_at(
        db: &Database,
        service_id: &str,
        host: &str,
        path: &str,
        created_at: &str,
    ) {
        let conn = db.conn().expect("数据库连接应可用");
        conn.execute(
            "INSERT INTO connection_events (
               service_id, protocol, remote_addr, target_addr, event_type,
               method, host, path, status_code, bytes_in, bytes_out,
               duration_ms, error, created_at
             )
             VALUES (?1, 'http', '127.0.0.1:50000', '127.0.0.1:19090',
                     'request', 'GET', ?2, ?3, 200, 10, 20, 30, '', ?4)",
            params![service_id, host, path, created_at],
        )
        .expect("连接日志应写入成功");
    }
}
