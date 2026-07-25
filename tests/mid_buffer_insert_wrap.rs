use termharness::{error::Result, scenario};

#[test]
fn mid_buffer_insert_wrap() -> Result<()> {
    let run = scenario::run_document(include_str!("../examples/mid_buffer_insert_wrap.zsh.th"))?;

    assert_eq!(run.records.len(), 4);
    Ok(())
}
