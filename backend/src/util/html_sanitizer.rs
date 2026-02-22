
pub fn sanitize_html(html: &str) -> String {
    ammonia::Builder::default()
        .add_tags(["img"])
        .add_generic_attributes(["loading"])
        .clean(html)
        .to_string()
}