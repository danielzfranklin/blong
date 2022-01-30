use anyhow::anyhow;
use std::{
    env,
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
};
use xshell::{cmd, Pushd};

fn main() -> Result<(), anyhow::Error> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let args = args.iter().map(|s| &**s).collect::<Vec<_>>();

    match &args[..] {
        ["flash"] => flash(),
        ["run"] => run_app(),
        ["check", "all"] => check_all(),
        ["test", "ada-gps"] => test_ada_gps(),
        ["test", "target"] => test_target(),
        ["traffic", "to-raw-rx", in_path, out_path] => traffic_to_raw_rx(in_path, out_path),
        ["traffic", "to-locus-bin", in_path, out_path] => traffic_to_locus_bin(in_path, out_path),
        _ => Err(anyhow!("Unsupported")),
    }
}

fn traffic_to_raw_rx(in_path: &str, out_path: &str) -> Result<(), anyhow::Error> {
    let input = root_dir().join(in_path);
    let input = File::open(input)?;
    let input = BufReader::new(input);

    let output = root_dir().join(out_path);
    let output = File::options().create_new(true).write(true).open(output)?;
    let mut output = BufWriter::new(output);

    for line in input.lines() {
        let line = line?;
        let line = &line["00:00:00.000 ".len()..];
        if !line.starts_with("<") {
            continue;
        }
        let line = &line[1..];

        output.write_all(line.as_bytes())?;
        output.write_all(b"\r\n")?;
    }

    output.flush()?;

    Ok(())
}

fn traffic_to_locus_bin(in_path: &str, out_path: &str) -> Result<(), anyhow::Error> {
    let input = root_dir().join(in_path);
    let input = File::open(input)?;
    let input = BufReader::new(input);

    let mut bytes = Vec::<u8>::new();
    let mut seen_start = false;
    let mut data_line_i = 0;

    for line in input.lines() {
        let line = line?;
        let line = &line["00:00:00.000 ".len()..];
        if !line.starts_with("<") {
            continue;
        }
        let line = &line[1..];

        check_nmea_checksum(line)?;

        // Start
        if line.starts_with("$PMTKLOX,0") {
            seen_start = true;
            continue;
        }

        // Skip until see start
        if !seen_start {
            continue;
        }

        // End
        if line.starts_with("$PMTKLOX,2") {
            break;
        }

        // Not data
        if !line.starts_with("$PMTKLOX,1") {
            return Err(anyhow!("Expected PMTKLOX,1"));
        }

        let fields = &line["$PMTKLOX,1,".len()..line.len() - "*5B".len()]
            .split(",")
            .collect::<Vec<_>>();
        let n: usize = fields[0].parse()?;
        let data = fields[1..].join("");

        if n != data_line_i {
            return Err(anyhow!("Out-of-order PMTKLOX,1"));
        }
        data_line_i += 1;

        let data = hex::decode(&data)?;
        bytes.extend_from_slice(&data);
    }

    let output = root_dir().join(out_path);
    let output = File::options().create_new(true).write(true).open(output)?;
    let mut output = BufWriter::new(output);
    output.write_all(&bytes)?;
    output.flush()?;

    Ok(())
}

fn check_nmea_checksum(raw: &str) -> Result<(), anyhow::Error> {
    let line = &raw[1..raw.len() - "*FF".len()];
    let expected = &raw[raw.len() - "FF".len()..];

    let mut actual = 0;
    for byte in line.as_bytes() {
        actual ^= byte;
    }
    let actual = format!("{:02X}", actual);

    if expected == actual {
        Ok(())
    } else {
        Err(anyhow!("{} failed checksum, actual: {}", raw, actual))
    }
}

fn run_app() -> Result<(), anyhow::Error> {
    let _p = pushd_app()?;
    cmd!("cargo run").run()?;
    Ok(())
}

fn test_ada_gps() -> Result<(), anyhow::Error> {
    let _p = pushd_ada_gps()?;
    cmd!("cargo test").run()?;
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

fn pushd_ada_gps() -> Result<Pushd, anyhow::Error> {
    xshell::pushd(root_dir().join("ada_gps")).map_err(|e| e.into())
}

fn root_dir() -> PathBuf {
    let mut xtask_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    xtask_dir.pop();
    xtask_dir
}
