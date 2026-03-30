use actix_session::Session;
use actix_web::{web, HttpRequest, HttpResponse};
use sqlx::MySqlPool;
use tera::{Context, Tera};

use crate::config::Config;
use crate::csrf;
use crate::models::*;
use crate::services;
use crate::utils::crypto::*;
use crate::utils::pagination::*;
use crate::utils::render::*;
use crate::utils::validation::*;
use crate::utils::MAX_COUNT_UPPER;
use crate::{db_try, db_try_optional};

fn get_session_username(session: &Session) -> String {
    session
        .get::<String>("user")
        .unwrap_or(None)
        .unwrap_or_default()
}

fn get_session_role(session: &Session) -> String {
    session
        .get::<String>("role")
        .unwrap_or(None)
        .unwrap_or_else(|| "admin".to_string())
}

fn is_super_admin(session: &Session) -> bool {
    get_session_role(session) == "super"
}

pub async fn list_page(
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    query: web::Query<ListQuery>,
) -> HttpResponse {
    let base = &config.server.context_path;
    let username = get_session_username(&session);
    let csrf_token = csrf::ensure_csrf_token(&session);
    let keyword = query.keyword.clone().unwrap_or_default();
    let (page, offset) = calc_page_offset(query.page);
    log::debug!("list_page: page={page}, keyword={keyword:?}");

    let (total, records) = db_try!(
        services::qrcode::list_qrcodes(pool.get_ref(), &keyword, offset).await,
        &tmpl,
        base
    );

    let total_pages = calc_total_pages(total);

    let extract_hashes: std::collections::HashMap<String, String> = records
        .iter()
        .map(|r| {
            (
                r.uuid.clone(),
                generate_extract_hash(&r.uuid, &config.server.extract_salt),
            )
        })
        .collect();

    let display_texts: std::collections::HashMap<String, String> = records
        .iter()
        .map(|r| (r.uuid.clone(), truncate_display(&r.text_content)))
        .collect();

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("csrf_token", &csrf_token);
    ctx.insert("records", &records);
    ctx.insert("extract_hashes", &extract_hashes);
    ctx.insert("display_texts", &display_texts);
    ctx.insert("page", &page);
    ctx.insert("total_pages", &total_pages);
    ctx.insert("keyword", &keyword);
    ctx.insert("role", &get_session_role(&session));
    ctx.insert("ai_enabled", &config.ai.is_some());
    ctx.insert("active_nav", "qrcode");

    render_template(&tmpl, "list.html", &ctx)
}

pub async fn create_page(
    tmpl: web::Data<Tera>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;
    let username = get_session_username(&session);
    let csrf_token = csrf::ensure_csrf_token(&session);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("csrf_token", &csrf_token);
    ctx.insert("role", &get_session_role(&session));
    ctx.insert("ai_enabled", &config.ai.is_some());
    ctx.insert("active_nav", "qrcode");

    render_template(&tmpl, "create.html", &ctx)
}

pub async fn create_handler(
    form: web::Form<CreateForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let base = &config.server.context_path;
    let username = get_session_username(&session);
    let client_ip = get_client_ip(&req);

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return render_error(
            &tmpl,
            base,
            "CSRF 校验失败，请刷新页面重试",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    let (segments, text_content_json) = match validate_segments(&form.text_content) {
        Ok(result) => result,
        Err(msg) => {
            return render_error(&tmpl, base, msg, actix_web::http::StatusCode::BAD_REQUEST)
        }
    };

    let max_count = form.max_count.unwrap_or(5).clamp(1, MAX_COUNT_UPPER);
    let remark = form.remark.as_deref().filter(|s| !s.trim().is_empty());

    match services::qrcode::create(pool.get_ref(), &text_content_json, remark, max_count).await {
        Ok(uuid) => {
            let detail = format!("segments={}, max_count={max_count}", segments.len());
            services::audit::log_action(
                pool.get_ref(),
                &username,
                "create",
                Some(&uuid),
                Some(&detail),
                &client_ip,
            )
            .await;
            HttpResponse::Found()
                .insert_header(("Location", format!("{base}/")))
                .finish()
        }
        Err(e) => {
            log::warn!("QR code create failed: error={e}");
            render_error(
                &tmpl,
                base,
                "创建失败，请稍后重试",
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}

pub async fn download_image(
    path: web::Path<String>,
    pool: web::Data<MySqlPool>,
    config: web::Data<Config>,
) -> HttpResponse {
    let uuid = path.into_inner();
    // Sanitize UUID for Content-Disposition header safety
    let safe_uuid: String = uuid
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(36)
        .collect();
    log::info!("Download QR image");
    let hash = generate_extract_hash(&safe_uuid, &config.server.extract_salt);
    let url = format!(
        "{}{}/extract/{safe_uuid}/{hash}",
        config.server.public_host, config.server.context_path
    );

    // Fetch remark from DB
    let remark = match services::qrcode::get_by_uuid(pool.get_ref(), &safe_uuid).await {
        Ok(Some(record)) => record.remark,
        _ => None,
    };

    match services::qrcode::generate_qr_image(&url, remark.as_deref()) {
        Ok(png_data) => HttpResponse::Ok()
            .content_type("image/png")
            .insert_header((
                "Content-Disposition",
                format!("attachment; filename=\"qrcode-{safe_uuid}.png\""),
            ))
            .body(png_data),
        Err(e) => HttpResponse::InternalServerError().body(e),
    }
}

pub async fn delete_handler(
    path: web::Path<String>,
    form: web::Form<ActionForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;
    let username = get_session_username(&session);
    let client_ip = get_client_ip(&req);

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return render_error(
            &tmpl,
            base,
            "CSRF 校验失败，请刷新页面重试",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    if let Err(e) = services::qrcode::delete(pool.get_ref(), &uuid).await {
        log::warn!("Delete failed: error={e}");
        return render_error(
            &tmpl,
            base,
            "删除失败，请稍后重试",
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    services::audit::log_action(
        pool.get_ref(),
        &username,
        "delete",
        Some(&uuid),
        None,
        &client_ip,
    )
    .await;
    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/")))
        .finish()
}

pub async fn reset_handler(
    path: web::Path<String>,
    form: web::Form<ActionForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;
    let username = get_session_username(&session);
    let client_ip = get_client_ip(&req);

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return render_error(
            &tmpl,
            base,
            "CSRF 校验失败，请刷新页面重试",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    if let Err(e) = services::qrcode::reset_slots(pool.get_ref(), &uuid).await {
        log::warn!("Reset failed: error={e}");
        return render_error(
            &tmpl,
            base,
            "重置失败，请稍后重试",
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    services::audit::log_action(
        pool.get_ref(),
        &username,
        "reset",
        Some(&uuid),
        None,
        &client_ip,
    )
    .await;
    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/")))
        .finish()
}

pub async fn edit_page(
    path: web::Path<String>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;
    let username = get_session_username(&session);
    let csrf_token = csrf::ensure_csrf_token(&session);

    let record = db_try_optional!(
        services::qrcode::get_by_uuid(pool.get_ref(), &uuid).await,
        &tmpl,
        base,
        "二维码不存在"
    );

    let segments = parse_segments(&record.text_content);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("csrf_token", &csrf_token);
    ctx.insert("record", &record);
    ctx.insert("segments", &segments);
    ctx.insert("role", &get_session_role(&session));
    ctx.insert("ai_enabled", &config.ai.is_some());
    ctx.insert("active_nav", "qrcode");

    render_template(&tmpl, "edit.html", &ctx)
}

pub async fn edit_handler(
    path: web::Path<String>,
    form: web::Form<CreateForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;
    let username = get_session_username(&session);
    let client_ip = get_client_ip(&req);

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return render_error(
            &tmpl,
            base,
            "CSRF 校验失败，请刷新页面重试",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    let (_segments, text_content_json) = match validate_segments(&form.text_content) {
        Ok(result) => result,
        Err(msg) => {
            return render_error(&tmpl, base, msg, actix_web::http::StatusCode::BAD_REQUEST)
        }
    };

    let max_count = form.max_count.unwrap_or(5).clamp(1, MAX_COUNT_UPPER);
    let remark = form.remark.as_deref().filter(|s| !s.trim().is_empty());

    if let Err(e) =
        services::qrcode::update(pool.get_ref(), &uuid, &text_content_json, remark, max_count).await
    {
        log::warn!("QR code update failed: error={e}");
        return render_error(
            &tmpl,
            base,
            "更新失败，请稍后重试",
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    let detail = format!("max_count={max_count}");
    services::audit::log_action(
        pool.get_ref(),
        &username,
        "edit",
        Some(&uuid),
        Some(&detail),
        &client_ip,
    )
    .await;
    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/")))
        .finish()
}

pub async fn extract_logs_page(
    path: web::Path<String>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    query: web::Query<LogsQuery>,
) -> HttpResponse {
    let uuid = path.into_inner();
    let base = &config.server.context_path;
    let username = get_session_username(&session);
    let (page, offset) = calc_page_offset(query.page);
    let list_page = query.list_page.unwrap_or(1);
    let list_keyword = query.list_keyword.clone().unwrap_or_default();

    let record = db_try_optional!(
        services::qrcode::get_by_uuid(pool.get_ref(), &uuid).await,
        &tmpl,
        base,
        "二维码不存在"
    );

    let (total, logs) = db_try!(
        services::qrcode::list_extract_logs(pool.get_ref(), record.id, offset).await,
        &tmpl,
        base
    );

    let total_pages = calc_total_pages(total);
    let display_text = truncate_display(&record.text_content);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("record", &record);
    ctx.insert("display_text", &display_text);
    ctx.insert("logs", &logs);
    ctx.insert("page", &page);
    ctx.insert("total_pages", &total_pages);
    ctx.insert("list_page", &list_page);
    ctx.insert("list_keyword", &list_keyword);
    ctx.insert("role", &get_session_role(&session));
    ctx.insert("ai_enabled", &config.ai.is_some());
    ctx.insert("active_nav", "qrcode");

    render_template(&tmpl, "logs.html", &ctx)
}

// ---- AI 评论生成 ----

pub async fn ai_generate_page(
    tmpl: web::Data<Tera>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;

    if config.ai.is_none() {
        return render_error(
            &tmpl,
            base,
            "AI 功能未配置",
            actix_web::http::StatusCode::NOT_FOUND,
        );
    }

    let username = get_session_username(&session);
    let csrf_token = csrf::ensure_csrf_token(&session);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("csrf_token", &csrf_token);
    ctx.insert("role", &get_session_role(&session));
    ctx.insert("ai_enabled", &true);
    ctx.insert("active_nav", "ai");

    render_template(&tmpl, "ai_generate.html", &ctx)
}

pub async fn ai_generate_handler(
    body: web::Json<AiGenerateRequest>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    if !csrf::validate_csrf_token(&session, &body.csrf_token) {
        return HttpResponse::Forbidden()
            .json(serde_json::json!({"status": "error", "message": "CSRF 校验失败"}));
    }

    let ai_config = match &config.ai {
        Some(c) => c,
        None => {
            return HttpResponse::BadRequest()
                .json(serde_json::json!({"status": "error", "message": "AI 功能未配置"}));
        }
    };

    let topic = body.topic.trim();
    if topic.is_empty() {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({"status": "error", "message": "主题不能为空"}));
    }

    let count = body.count.unwrap_or(10).clamp(1, 50);
    let style = body.style.as_deref().unwrap_or("").trim();
    let examples = body.examples.as_deref().unwrap_or("").trim();

    match services::ai::generate_comments(ai_config, topic, count, style, examples).await {
        Ok(comments) => {
            HttpResponse::Ok().json(serde_json::json!({"status": "ok", "comments": comments}))
        }
        Err(e) => {
            log::warn!("AI generate failed: {e}");
            HttpResponse::Ok().json(serde_json::json!({"status": "error", "message": e}))
        }
    }
}

pub async fn ai_create_handler(
    form: web::Form<AiCreateForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let base = &config.server.context_path;
    let username = get_session_username(&session);
    let client_ip = get_client_ip(&req);

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return render_error(
            &tmpl,
            base,
            "CSRF 校验失败，请刷新页面重试",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    let (segments, text_content_json) = match validate_segments(&form.comments) {
        Ok(result) => result,
        Err(msg) => {
            return render_error(&tmpl, base, msg, actix_web::http::StatusCode::BAD_REQUEST)
        }
    };

    let max_count = form
        .max_count
        .unwrap_or(segments.len() as u32)
        .clamp(1, MAX_COUNT_UPPER);
    let remark = form.remark.as_deref().filter(|s| !s.trim().is_empty());

    match services::qrcode::create(pool.get_ref(), &text_content_json, remark, max_count).await {
        Ok(uuid) => {
            let detail = format!("segments={}, max_count={max_count}", segments.len());
            services::audit::log_action(
                pool.get_ref(),
                &username,
                "ai_create",
                Some(&uuid),
                Some(&detail),
                &client_ip,
            )
            .await;
            HttpResponse::Found()
                .insert_header(("Location", format!("{base}/")))
                .finish()
        }
        Err(e) => {
            log::warn!("QR code AI create failed: error={e}");
            render_error(
                &tmpl,
                base,
                "创建失败，请稍后重试",
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}

// ---- 审计日志 ----

pub async fn audit_logs_page(
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    query: web::Query<AuditLogsQuery>,
) -> HttpResponse {
    let base = &config.server.context_path;

    if !is_super_admin(&session) {
        return render_error(
            &tmpl,
            base,
            "权限不足，仅超级管理员可访问",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    let username = get_session_username(&session);
    let action_filter = query.action.clone().unwrap_or_default();
    let keyword = query.keyword.clone().unwrap_or_default();

    let (total, logs) = db_try!(
        services::audit::list_logs(pool.get_ref(), &action_filter, &keyword, query.page).await,
        &tmpl,
        base
    );

    let (page, _offset) = calc_page_offset(query.page);
    let total_pages = calc_total_pages(total);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("role", &get_session_role(&session));
    ctx.insert("logs", &logs);
    ctx.insert("page", &page);
    ctx.insert("total_pages", &total_pages);
    ctx.insert("action_filter", &action_filter);
    ctx.insert("keyword", &keyword);
    ctx.insert("ai_enabled", &config.ai.is_some());
    ctx.insert("active_nav", "audit");

    render_template(&tmpl, "audit_logs.html", &ctx)
}

// ---- 用户管理 (super admin only) ----

pub async fn users_page(
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;

    if !is_super_admin(&session) {
        return render_error(
            &tmpl,
            base,
            "权限不足，仅超级管理员可访问",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    let username = get_session_username(&session);
    let csrf_token = csrf::ensure_csrf_token(&session);

    let users = db_try!(
        services::user::list_users(pool.get_ref()).await,
        &tmpl,
        base
    );

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("role", &get_session_role(&session));
    ctx.insert("csrf_token", &csrf_token);
    ctx.insert("users", &users);
    ctx.insert("ai_enabled", &config.ai.is_some());
    ctx.insert("active_nav", "users");

    render_template(&tmpl, "users.html", &ctx)
}

pub async fn create_user_handler(
    form: web::Form<CreateUserForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let base = &config.server.context_path;

    if !is_super_admin(&session) {
        return render_error(
            &tmpl,
            base,
            "权限不足",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return render_error(
            &tmpl,
            base,
            "CSRF 校验失败，请刷新页面重试",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    let username = get_session_username(&session);
    let client_ip = get_client_ip(&req);

    if let Err(msg) =
        services::user::create_user(pool.get_ref(), &form.username, &form.password).await
    {
        return render_error(&tmpl, base, &msg, actix_web::http::StatusCode::BAD_REQUEST);
    }

    let detail = format!("new_user={}", form.username.trim());
    services::audit::log_action(
        pool.get_ref(),
        &username,
        "create_user",
        None,
        Some(&detail),
        &client_ip,
    )
    .await;

    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/users")))
        .finish()
}

pub async fn toggle_user_handler(
    form: web::Form<ToggleUserForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let base = &config.server.context_path;

    if !is_super_admin(&session) {
        return render_error(
            &tmpl,
            base,
            "权限不足",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return render_error(
            &tmpl,
            base,
            "CSRF 校验失败，请刷新页面重试",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    let username = get_session_username(&session);
    let client_ip = get_client_ip(&req);

    if let Err(e) = services::user::toggle_user(pool.get_ref(), form.id, form.is_active).await {
        log::warn!("Toggle user failed: {e}");
        return render_error(
            &tmpl,
            base,
            "操作失败",
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    let detail = format!("user_id={}, is_active={}", form.id, form.is_active);
    services::audit::log_action(
        pool.get_ref(),
        &username,
        "toggle_user",
        None,
        Some(&detail),
        &client_ip,
    )
    .await;

    HttpResponse::Found()
        .insert_header(("Location", format!("{base}/users")))
        .finish()
}

// ---- 修改密码 (admin users only) ----

pub async fn change_password_page(
    tmpl: web::Data<Tera>,
    session: Session,
    config: web::Data<Config>,
) -> HttpResponse {
    let base = &config.server.context_path;
    let role = get_session_role(&session);

    // Only DB admins can change password (super admin uses config file)
    if role != "admin" {
        return render_error(
            &tmpl,
            base,
            "超级管理员的密码请在配置文件中修改",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    let username = get_session_username(&session);
    let csrf_token = csrf::ensure_csrf_token(&session);

    let mut ctx = Context::new();
    ctx.insert("base", base);
    ctx.insert("username", &username);
    ctx.insert("role", &role);
    ctx.insert("csrf_token", &csrf_token);
    ctx.insert("ai_enabled", &config.ai.is_some());
    ctx.insert("active_nav", "password");

    render_template(&tmpl, "change_password.html", &ctx)
}

pub async fn change_password_handler(
    form: web::Form<ChangePasswordForm>,
    tmpl: web::Data<Tera>,
    pool: web::Data<MySqlPool>,
    session: Session,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let base = &config.server.context_path;
    let role = get_session_role(&session);

    if role != "admin" {
        return render_error(
            &tmpl,
            base,
            "超级管理员的密码请在配置文件中修改",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    if !csrf::validate_csrf_token(&session, &form.csrf_token) {
        return render_error(
            &tmpl,
            base,
            "CSRF 校验失败，请刷新页面重试",
            actix_web::http::StatusCode::FORBIDDEN,
        );
    }

    let username = get_session_username(&session);
    let client_ip = get_client_ip(&req);

    match services::user::change_password(
        pool.get_ref(),
        &username,
        &form.old_password,
        &form.new_password,
    )
    .await
    {
        Ok(()) => {
            services::audit::log_action(
                pool.get_ref(),
                &username,
                "change_password",
                None,
                None,
                &client_ip,
            )
            .await;
            // Purge session so user must re-login
            session.purge();
            HttpResponse::Found()
                .insert_header(("Location", format!("{base}/login")))
                .finish()
        }
        Err(msg) => render_error(&tmpl, base, &msg, actix_web::http::StatusCode::BAD_REQUEST),
    }
}
