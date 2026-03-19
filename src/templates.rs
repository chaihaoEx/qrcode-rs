use tera::Tera;

pub fn init_templates() -> Tera {
    Tera::new("templates/**/*.html").expect("Failed to initialize Tera templates")
}
