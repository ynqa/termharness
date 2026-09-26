use termharness::{error::Result, scenario};

#[test]
fn scrolls_the_viewport_without_sending_input_to_the_application() -> Result<()> {
    scenario::run_document(include_str!("../examples/scrollback.zsh.th"))?;
    Ok(())
}
