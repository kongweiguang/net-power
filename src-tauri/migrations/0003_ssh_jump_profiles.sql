-- @author kongweiguang
-- 为 SSH Profile 增加可选跳板引用；多级跳板由 profile 引用链表达。

ALTER TABLE ssh_profiles ADD COLUMN jump_profile_id TEXT REFERENCES ssh_profiles(id);

CREATE INDEX IF NOT EXISTS idx_ssh_profiles_jump_profile_id ON ssh_profiles(jump_profile_id);
