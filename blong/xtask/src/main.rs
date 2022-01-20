use std::{env, path::PathBuf};
use xshell::{cmd, Pushd};

fn main() -> Result<(), anyhow::Error> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let args = args.iter().map(|s| &**s).collect::<Vec<_>>();

    match &args[..] {
        ["flash"] => flash(),
        ["run"] => run_app(),
        ["check", "all"] => check_all(),
        ["test", "target"] => test_target(),
        _ => {
            println!("Unsupported");
            Ok(())
        }
    }
}

fn run_app() -> Result<(), anyhow::Error> {
    let _p = pushd_app()?;
    cmd!("cargo run").run()?;
    Ok(())
}

fn test_target() -> Result<(), anyhow::Error> {
    let _p = pushd_cross()?;
    cmd!("cargo test -p self-tests").run()?;
    Ok(())
}

fn flash() -> Result<(), anyhow::Error> {
    let _p = pushd_app()?;
    cmd!("cargo flash --chip rp2040 --release").run()?;
    Ok(())
}

fn check_all() -> Result<(), anyhow::Error> {
    check_root()?;
    check_cross()?;
    Ok(())
}

fn check_root() -> Result<(), anyhow::Error> {
    let _p = pushd_root()?;
    cmd!("cargo check").run()?;
    Ok(())
}

fn check_cross() -> Result<(), anyhow::Error> {
    let _p = pushd_cross()?;
    cmd!("cargo check").run()?;
    Ok(())
}

fn pushd_root() -> Result<Pushd, anyhow::Error> {
    xshell::pushd(root_dir()).map_err(|e| e.into())
}

fn pushd_cross() -> Result<Pushd, anyhow::Error> {
    xshell::pushd(root_dir().join("cross")).map_err(|e| e.into())
}

fn pushd_app() -> Result<Pushd, anyhow::Error> {
    xshell::pushd(root_dir().join("cross").join("app")).map_err(|e| e.into())
}

fn root_dir() -> PathBuf {
    let mut xtask_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    xtask_dir.pop();
    xtask_dir
}
