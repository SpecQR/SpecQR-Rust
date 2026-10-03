"""Development-only process bridge to compiled, dependency-free Rust.

Python only orchestrates and verifies results. QR candidates are always generated
by a Rust executable compiled from the selected candidate sources (or an exact
explicitly selected built executable), never by a reference encoder.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

CARGO = os.environ.get('SPECQR_CARGO', 'cargo')
BINARY = None
DRIVER = Path(__file__).resolve().parents[2] / 'src/bin/conformance.rs'
_COMPILED = {}
_TEMP = []


def configure(cargo=None, binary=None):
    global CARGO, BINARY
    CARGO, BINARY = cargo or os.environ.get('SPECQR_CARGO', 'cargo'), Path(binary).resolve() if binary else None


def _binary(candidate):
    root = Path(candidate).resolve()
    if BINARY:
        if not BINARY.is_file():
            raise RuntimeError('Selected Rust binary does not exist: ' + str(BINARY))
        return BINARY
    files = sorted((root / 'src').rglob('*.rs')) + [root / 'Cargo.toml', root / 'Cargo.lock']
    if not files or any(not p.is_file() for p in files):
        raise RuntimeError('Candidate Rust sources/Cargo.lock missing: ' + str(root))
    fingerprint = tuple((str(p.relative_to(root)), hashlib.sha256(p.read_bytes()).hexdigest()) for p in files)
    key = (CARGO, fingerprint)
    if key not in _COMPILED:
        metadata = json.loads(subprocess.check_output([CARGO, 'metadata', '--offline', '--locked', '--format-version', '1', '--manifest-path', str(root / 'Cargo.toml')], text=True))
        packages = metadata.get('packages', [])
        if len(packages) != 1 or packages[0].get('name') != 'specqr' or packages[0].get('dependencies'):
            raise RuntimeError('Candidate must have zero Cargo dependencies, including development dependencies')
        temporary = tempfile.TemporaryDirectory(prefix='specqr-rust-conformance-')
        _TEMP.append(temporary)
        target = Path(temporary.name)
        command = [CARGO, 'build', '--offline', '--locked', '--release', '--features', 'conformance', '--bin', 'conformance', '--target-dir', str(target), '--manifest-path', str(root / 'Cargo.toml')]
        process = subprocess.run(command, capture_output=True, text=True, timeout=240)
        if process.returncode:
            raise RuntimeError('Rust candidate compilation failed:\n' + process.stdout + process.stderr)
        binary = target / 'release' / ('conformance.exe' if os.name == 'nt' else 'conformance')
        if not binary.is_file():
            raise RuntimeError('Cargo did not produce expected Rust conformance binary')
        _COMPILED[key] = binary.resolve()
    return _COMPILED[key]


def generate(candidate, requests, fault=None):
    binary = _binary(candidate)
    env = os.environ.copy()
    for name in ('NODE_OPTIONS', 'NODE_PATH', 'PYTHONPATH', 'CLASSPATH', 'LD_PRELOAD', 'DYLD_INSERT_LIBRARIES', 'SPECQR_TEST_FAULT'):
        env.pop(name, None)
    if fault:
        env['SPECQR_TEST_FAULT'] = fault
    process = subprocess.run([str(binary), '--json-lines'],
                             input=''.join(json.dumps(r, ensure_ascii=True) + '\n' for r in requests),
                             capture_output=True, text=True, encoding='utf-8', env=env, timeout=900)
    if process.returncode:
        raise RuntimeError(f'Rust candidate process failed ({process.returncode}): {process.stderr[-4000:]}')
    try:
        results = [json.loads(line) for line in process.stdout.splitlines()]
    except json.JSONDecodeError as error:
        raise RuntimeError(f'Invalid Rust JSON-lines response: {process.stdout[:1000]!r}; stderr={process.stderr[-1000:]!r}') from error
    if len(results) != len(requests):
        raise RuntimeError(f'Rust response count mismatch: {len(results)} != {len(requests)}')
    return results


def matrix_hash(matrix):
    return hashlib.sha256(''.join(matrix).encode('ascii')).hexdigest()


def verify_identity(candidate, identity, nonce):
    binary = _binary(candidate)
    fingerprint = hashlib.sha256(binary.read_bytes()).hexdigest()
    if (identity.get('nonce') != nonce or identity.get('pid') == os.getpid()
            or not isinstance(identity.get('pid'), int)
            or identity.get('language') != 'Rust'
            or Path(identity.get('executable', '')).resolve() != binary
            or identity.get('binarySha256') != fingerprint
            or identity.get('runtimeDependencies') != []
            or identity.get('packageFilesSha256') != {binary.name: fingerprint}):
        raise RuntimeError(f'Candidate Rust process/executable identity check failed: {identity}')
