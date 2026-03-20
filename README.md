# qrcode-rs

基于 Rust + Actix-web 的二维码生成与提取管理 Web 应用。

## 功能

- **管理后台**：创建/编辑/删除二维码，多段文字内容，搜索与分页
- **多管理员支持**：超级管理员（配置文件）+ 普通管理员（数据库），角色权限隔离
- **二维码图片下载**：蓝紫渐变风格 PNG，备注文字自动标注在图片底部
- **扫码提取**：browser_id 槽位模型，每个浏览器顺序分配一段内容，幂等返回
- **提取记录**：记录每次提取的客户端 IP、browser_id、段落索引和时间
- **AI 评论生成**：对接大模型 API，批量生成评论并一键创建二维码
- **审计日志**：记录所有管理操作（超级管理员可查看）
- **安全防护**：
  - CSRF 双层防护（Token + SameSite=Strict）
  - HMAC-SHA256 链接签名（64-bit，常量时间比较）
  - 登录频率限制（10 次/5 分钟/IP）
  - 数据库用户连续 5 次密码错误自动锁定 30 分钟
  - Session 安全属性（HttpOnly、Secure、8h 过期）
  - 事务行锁防并发超额分配
- **HTTPS 支持**：可选 Rustls TLS 终止 + HTTP→HTTPS 自动重定向

## 环境要求

- Rust 1.70+
- MySQL 5.7+ / 8.0

## 快速开始

### 1. 初始化数据库

```bash
mysql -u root -p < sql/init.sql
mysql -u root -p qrcode_db < sql/migrations/001_add_audit_logs.sql
mysql -u root -p qrcode_db < sql/migrations/002_add_admin_users.sql
```

### 2. 配置

```bash
cp config.example.toml config.toml
```

编辑 `config.toml`：

```toml
[server]
host = "127.0.0.1"
port = 8080
secret_key = "至少64字符的随机字符串"
context_path = ""                          # 虚拟目录前缀，如 "/qrcode"
public_host = "http://127.0.0.1:8080"     # 二维码中的外部访问地址
extract_salt = "替换为随机盐值"
# legacy_hash_support = true              # 兼容旧版 8 字符 HMAC 哈希
# https_port = 8443                       # 启用 HTTPS
# tls_cert = "/path/to/cert.pem"
# tls_key = "/path/to/key.pem"

[admin]
username = "admin"
password_hash = "$2b$12$..."              # 见下方生成方法

[database]
url = "mysql://root:password@localhost:3306/qrcode_db"
# max_connections = 10                    # 连接池大小
# timezone = "+08:00"                     # 会话时区
```

生成管理员密码哈希：

```bash
cargo run -- hash-password your_password
```

### 3. 构建与运行

```bash
cargo build --release
cargo run
```

启用调试日志：

```bash
RUST_LOG=debug cargo run
```

### 4. 测试

```bash
cargo test     # 运行 34 个单元测试
```

## 项目结构

```
qrcode-rs/
├── Cargo.toml              # 项目依赖
├── config.example.toml     # 配置模板
├── sql/
│   ├── init.sql             # 建库建表脚本
│   └── migrations/          # 增量迁移文件
├── src/
│   ├── main.rs              # 应用入口、HTTPS/HTTP 服务配置
│   ├── config.rs            # 配置加载与校验
│   ├── db.rs                # 数据库连接池初始化
│   ├── middleware.rs         # 认证中间件 (AuthGuard)
│   ├── csrf.rs              # CSRF Token 管理
│   ├── rate_limit.rs        # 登录频率限制
│   ├── templates.rs         # 模板引擎初始化
│   ├── models/
│   │   ├── domain.rs        # 数据结构（QrCodeRecord, AdminUser 等）
│   │   └── request.rs       # 表单/查询结构体
│   ├── services/
│   │   ├── qrcode.rs        # 二维码 CRUD + 图片生成
│   │   ├── extract.rs       # 槽位分配
│   │   ├── audit.rs         # 审计日志
│   │   ├── user.rs          # 多管理员 CRUD + 登录验证
│   │   └── ai.rs            # AI 评论生成
│   ├── utils/
│   │   ├── crypto.rs        # HMAC 签名
│   │   ├── pagination.rs    # 分页计算
│   │   ├── render.rs        # 模板渲染助手、db_try! 宏
│   │   └── validation.rs    # 输入校验、IP 提取
│   └── routes/
│       ├── mod.rs            # 路由注册
│       ├── auth.rs           # 登录/登出（支持多管理员）
│       ├── admin.rs          # 管理端 CRUD + 用户管理 + 权限控制
│       └── extract.rs        # 公开提取接口
├── templates/                # Tera HTML 模板
└── static/                   # 静态资源（CSS、favicon）
```

## 部署

交叉编译 Linux 静态链接二进制：

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

使用 systemd 管理服务，配置 HTTPS 证书路径后即可启用 TLS。

## License

MIT
