#!/usr/bin/env python3
"""Challa local release build and publishing script.

Usage:
    uv run python scripts/release.py [--dry-run]
"""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import subprocess
import sys


def find_credential_manager() -> Path | None:
    home = Path.home()
    candidates = [
        home / ".gemini" / "skills" / "using-credentials" / "scripts" / "credential_manager.py",
        home / ".agents" / "skills" / "using-credentials" / "scripts" / "credential_manager.py",
    ]
    for path in candidates:
        if path.exists():
            return path
    return None


def read_workspace_version(root: Path) -> str:
    cargo_toml = (root / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(r'\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"', cargo_toml)
    if not m:
        raise ValueError("Cargo.toml에서 [workspace.package] version을 찾을 수 없습니다.")
    return m.group(1).strip()


def read_package_json_version(root: Path) -> str:
    package_json = json.loads((root / "package.json").read_text(encoding="utf-8"))
    return str(package_json.get("version", "")).strip()


def read_tauri_conf_version(root: Path) -> str:
    tauri_conf = json.loads((root / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))
    return str(tauri_conf.get("version", "")).strip()


def verify_version_consistency(root: Path) -> str:
    v_cargo = read_workspace_version(root)
    v_package = read_package_json_version(root)
    v_tauri = read_tauri_conf_version(root)

    if not (v_cargo == v_package == v_tauri):
        raise ValueError(
            f"버전 불일치 발생:\n"
            f"  - Cargo.toml: {v_cargo}\n"
            f"  - package.json: {v_package}\n"
            f"  - tauri.conf.json: {v_tauri}\n"
            f"세 파일의 버전을 동일하게 맞춰주세요."
        )
    return v_cargo


def load_signing_credentials() -> tuple[str, str]:
    cred_script = find_credential_manager()
    if not cred_script:
        raise RuntimeError("using-credentials 스킬의 credential_manager.py를 찾을 수 없습니다.")

    sys.path.insert(0, str(cred_script.parent))
    try:
        from credential_manager import read_secret, credential_exists

        if not credential_exists("challa-signing_key"):
            raise RuntimeError(
                "Windows 자격 증명 'challa-signing_key'가 없습니다. 먼저 서명 키를 등록해 주세요."
            )
        if not credential_exists("challa-signing_password"):
            raise RuntimeError(
                "Windows 자격 증명 'challa-signing_password'가 없습니다. 먼저 서명 키 비밀번호를 등록해 주세요."
            )

        key = read_secret("challa-signing_key")
        password = read_secret("challa-signing_password")
        return key, password
    except Exception as e:
        raise RuntimeError(f"서명 자격 증명을 읽지 못했습니다: {e}") from e


def find_artifacts(bundle_dir: Path, version: str) -> tuple[Path, Path]:
    """Return this version's NSIS installer and its updater signature.

    Only the installer is published. The bare target/release/challa.exe is never uploaded.
    """
    nsis_dir = bundle_dir / "nsis"
    installer = nsis_dir / f"Challa_{version}_x64-setup.exe"
    sig_file = nsis_dir / f"{installer.name}.sig"
    for path in (installer, sig_file):
        if not path.is_file():
            raise FileNotFoundError(f"릴리스 산출물이 없습니다: {path}")
    return installer, sig_file


def generate_latest_json(
    version: str, installer_file: Path, sig_file: Path, output_path: Path
) -> dict:
    signature = sig_file.read_text(encoding="utf-8").strip()
    tag_name = f"v{version}"
    download_url = (
        f"https://github.com/cloim/Challa/releases/download/{tag_name}/{installer_file.name}"
    )

    manifest = {
        "version": version,
        "notes": f"Challa {tag_name} release",
        "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": {
            "windows-x86_64": {
                "signature": signature,
                "url": download_url,
            }
        },
    }

    output_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False), encoding="utf-8")
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(description="Challa release build and publishing script")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Build updater artifacts and create latest.json without publishing to GitHub",
    )
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent
    os.chdir(root)

    print("[1/5] 버전 일치 여부 확인...")
    version = verify_version_consistency(root)
    tag_name = f"v{version}"
    print(f"      버전 일치 확인 완료: {version} (태그: {tag_name})")

    print("[2/5] Windows 자격 증명 관리자에서 서명 키 확인...")
    private_key, key_password = load_signing_credentials()
    print("      서명 자격 증명 확인 완료")

    print("[3/5] pnpm tauri build 실행 중 (updater 산출물 포함)...")
    env = os.environ.copy()
    env["TAURI_SIGNING_PRIVATE_KEY"] = private_key
    env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = key_password

    build_cmd = ["cmd", "/c", "pnpm", "tauri", "build"]
    res = subprocess.run(build_cmd, env=env, cwd=root)
    if res.returncode != 0:
        print(f"ERROR: tauri build failed with exit code {res.returncode}", file=sys.stderr)
        sys.exit(res.returncode)

    print("[4/5] 번들 산출물 및 .sig 파일 탐색 및 latest.json 생성...")
    bundle_dir = root / "target" / "release" / "bundle"
    if not bundle_dir.exists():
        bundle_dir = root / "src-tauri" / "target" / "release" / "bundle"

    installer, sig_file = find_artifacts(bundle_dir, version)
    latest_json_path = bundle_dir / "latest.json"
    manifest = generate_latest_json(version, installer, sig_file, latest_json_path)
    print(f"      설치 파일: {installer.name} ({installer.stat().st_size:,} bytes)")
    print(f"      서명 파일: {sig_file.name}")
    print(f"      latest.json 생성 완료: {latest_json_path}")
    print(f"      다운로드 URL: {manifest['platforms']['windows-x86_64']['url']}")

    if args.dry_run:
        print("\n[5/5] --dry-run 모드: GitHub 릴리스 생성 및 업로드를 건너뜁니다.")
        print("=== latest.json 내용 ===")
        print(json.dumps(manifest, indent=2))
        print("=== Dry-run 완료 ===")
        sys.exit(0)

    print(f"[5/5] GitHub Release {tag_name} 생성 및 에셋 업로드...")
    gh_cmd = [
        "gh",
        "release",
        "create",
        tag_name,
        str(installer),
        str(sig_file),
        str(latest_json_path),
        "--title",
        tag_name,
        "--notes",
        f"Challa release {tag_name}",
    ]
    gh_res = subprocess.run(gh_cmd, cwd=root)
    if gh_res.returncode != 0:
        print(f"ERROR: gh release create failed with exit code {gh_res.returncode}", file=sys.stderr)
        sys.exit(gh_res.returncode)

    print(f"Challa {tag_name} 릴리스 배포가 성공적으로 완료되었습니다.")


if __name__ == "__main__":
    main()
