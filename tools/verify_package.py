#!/usr/bin/env python3
"""Rebuild a Cargo package in isolation; install CLI and test a path consumer."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]

def run(args, cwd=ROOT, env=None):
    result = subprocess.run(args, cwd=cwd, env=env, text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError(f"{args!r} failed\n{result.stdout}\n{result.stderr}")
    return result.stdout

def main():
    metadata = json.loads(run(["cargo", "metadata", "--format-version", "1", "--no-deps", "--offline"]))
    package = metadata["packages"][0]
    assert package["name"] == "specqr"
    assert package["dependencies"] == [], "Cargo dependency graph must be empty"
    run(["cargo", "package", "--allow-dirty", "--offline"])
    artifact = Path(metadata["target_directory"]) / "package" / f"specqr-{package['version']}.crate"
    with tempfile.TemporaryDirectory(prefix="specqr-cargo-consumer-") as tmp:
        tmp = Path(tmp)
        with tarfile.open(artifact) as archive:
            for member in archive.getmembers():
                p = Path(member.name)
                assert not p.is_absolute() and ".." not in p.parts
                assert member.isfile() or member.isdir(), "Archive must not contain links or special files"
                assert not any(part in {"target", "node_modules", ".git"} for part in p.parts)
            archive.extractall(tmp, filter="data")
        source = tmp / f"specqr-{package['version']}"
        env = dict(os.environ, CARGO_TARGET_DIR=str(tmp / "target"))
        run(["cargo", "test", "--offline", "--all-targets"], source, env)
        run(["cargo", "test", "--offline", "--doc"], source, env)
        run(["cargo", "install", "--offline", "--locked", "--path", str(source), "--bin", "specqr", "--root", str(tmp / "installed")], source, env)
        executable = tmp / "installed" / "bin" / ("specqr.exe" if os.name == "nt" else "specqr")
        assert run([str(executable), "--package-version"], tmp).strip() == package["version"]
        assert run([str(executable), "public detached consumer", "--format", "json"], tmp).startswith("{")
        consumer = tmp / "consumer"
        (consumer / "src").mkdir(parents=True)
        # TOML accepts a quoted path after backslash escaping through JSON.
        (consumer / "Cargo.toml").write_text('[package]\nname="detached-consumer"\nversion="0.0.0"\nedition="2024"\n[dependencies]\nspecqr={path=' + json.dumps(str(source)) + '}\n', encoding="utf-8")
        (consumer / "src" / "main.rs").write_text('''
use specqr::{Options,Segment,Ecc};
fn main()->specqr::Result<()> {
 let qr=specqr::generate("Detached 日本語 consumer",&Options::default())?;
 assert!(qr.size()>=21); assert!(qr.to_png()?.starts_with(&[137,80,78,71]));
 let raw=specqr::generate_bytes(&[0,255,128],&Options::default())?;
 assert_eq!(raw.segments()[0].logical_bytes(),&[0,255,128]);
 let s=specqr::generate_segments(&[Segment::eci(26)?,Segment::utf8("é😀")?],&Options::default())?;
 assert_eq!(s.segments()[0].assignment(),Some(26));
 assert!(specqr::get_capacity(40,Ecc::L,Some(specqr::Mode::Byte),0)?.maximum().unwrap()>2900);
 Ok(())
}
''', encoding="utf-8")
        run(["cargo", "run", "--offline", "--quiet"], consumer, env)
    print(json.dumps({"package": package["name"], "version": package["version"], "dependencies": [], "package_verified": True, "detached_tests": True, "installed_cli": True, "path_consumer": True}))

if __name__ == "__main__":
    main()
