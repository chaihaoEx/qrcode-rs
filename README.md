# qrcode-rs

基于 Rust + Actix-web 的二维码生成与提取管理 Web 应用。

## 功能

- 管理后台：创建二维码、设置最大提取次数、搜索与分页
- 二维码图片下载（PNG 格式）
- 扫码提取：HMAC 签名防篡改，原子计数防超额
- 提取记录：记录每次提取的客户端 IP 和时间
- 会话认证：bcrypt 密码验证 + Cookie Session

## 环境要求

- Rust 1.70+
- MySQL 5.7+

## 快速开始

### 1. 初始化数据库

```bash
mysql -u root -p < sql/init.sql
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

[admin]
username = "admin"
password_hash = "$2b$12$..."              # 见下方生成方法

[database]
url = "mysql://root:password@localhost:3306/qrcode_db"
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

## 项目结构

```
qrcode-rs/
├── Cargo.toml              # 项目依赖
├── config.example.toml     # 配置模板
├── sql/
│   └── init.sql            # 建库建表脚本
├── src/
│   ├── main.rs             # 应用入口
│   ├── config.rs           # 配置加载
│   ├── db.rs               # 数据库连接池
│   ├── middleware.rs        # 认证中间件
│   ├── templates.rs         # 模板引擎初始化
│   └── routes/
│       ├── mod.rs           # 路由注册
│       ├── auth.rs          # 登录/登出
│       └── qrcode.rs        # 二维码业务逻辑
├── templates/               # Tera HTML 模板
└── static/                  # 静态资源
```

## License

MIT
