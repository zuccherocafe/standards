//! Common static-serving scenario through the public boundary.

use std::{error::Error, fs};

use makiatto::{config::Config, web::WebServer};

#[test]
fn alias_request_serves_a_domain_index() -> Result<(), Box<dyn Error>> {
    let root = format!(".cache/static-scenario-{}", std::process::id());
    fs::create_dir_all(format!("{root}/example.test"))?;
    fs::write(format!("{root}/example.test/index.html"), b"site home")?;
    let config = Config::from_toml(&format!(
        "[web]\nstatic_dir = {root:?}\n[domains]\n'alias.test' = 'example.test'\n"
    ))?;
    let response = WebServer::new(config).serve("alias.test:80", "/", "")?;
    assert_eq!(response.body, b"site home");
    assert_eq!(response.content_type, "text/html");
    fs::remove_dir_all(root)?;
    Ok(())
}
