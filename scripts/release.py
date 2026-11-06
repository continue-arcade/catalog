#!/usr/bin/env python3
"""固定输入 → 官方 Cart 工具打包与校验 → 不可覆盖的 GitHub Release。"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CART_ID = "org.continue.system.catalog"
VERSION = re.compile(r"(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*)?")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def run(*arguments):
    result = subprocess.run([str(value) for value in arguments], cwd=ROOT, capture_output=True, text=True, timeout=120, check=False)
    require(result.returncode == 0, f"{Path(arguments[0]).name} 执行失败（{result.returncode}）：{result.stderr[-2000:]}")
    return result.stdout


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_version(version):
    require(isinstance(version, str) and len(version) <= 100 and VERSION.fullmatch(version), "版本必须是明确 SemVer")


def verify_tag_input(version):
    tag = "v" + version
    for name in ["release.json", "source/cartfile.toml", "source/system/catalog/continue.toml", "source/system/catalog/catalog.json"]:
        path = f"snapshots/{version}/{name}"
        require(run("git", "show", f"{tag}:{path}").encode("utf-8") == (ROOT / path).read_bytes(), f"当前输入与不可变 tag 不符：{name}")


def find_release(repository, tag):
    # /releases/tags/{tag} 不可靠地暴露草稿；通过有写权限的列表找到精确 tag，后续按 ID 操作。
    for page in range(1, 21):
        batch = json.loads(run("gh", "api", f"repos/{repository}/releases?per_page=100&page={page}"))
        matches = [item for item in batch if item["tag_name"] == tag]
        require(len(matches) <= 1, "远端存在重复 tag 的 Release")
        if matches:
            return matches[0]
        if len(batch) < 100:
            return None
    raise ValueError("Release 列表超过安全分页上限")


def verify_toolchain():
    manifest = json.loads((ROOT / "toolchain/source.json").read_text())
    actual = {str(path.relative_to(ROOT / "toolchain")) for folder in ["crates", "packages", "examples", "extensions"] for path in (ROOT / "toolchain" / folder).rglob("*") if path.is_file()}
    require(actual == set(manifest["files"]), "工具链文件集合已变化")
    for name, expected in manifest["files"].items():
        path = ROOT / "toolchain" / name
        require(not path.is_symlink() and sha256(path) == expected, f"工具链来源摘要不符：{name}")


def build(version, output):
    validate_version(version)
    verify_toolchain()
    source_root = ROOT / "snapshots" / version
    source = source_root / "source"
    metadata = json.loads((source_root / "release.json").read_text())
    require(metadata["schema"] == "continue.catalog-release-input.v1" and metadata["cart_id"] == CART_ID and metadata["version"] == version, "输入身份或版本不符")
    files = {str(path.relative_to(source)) for path in source.rglob("*") if path.is_file()}
    require(files == {"cartfile.toml", "system/catalog/continue.toml", "system/catalog/catalog.json"}, "输入只允许系统字典和固定 manifests")
    require(not any(path.is_symlink() for path in source_root.rglob("*")), "输入不能包含符号链接")
    require(sha256(source / "system/catalog/catalog.json") == metadata["snapshot_sha256"], "固定字典摘要不符")
    output = Path(output).resolve()
    require(not output.exists(), "输出目录已存在，不能覆盖")
    output.parent.mkdir(parents=True, exist_ok=True)
    binary = os.environ.get("CONTINUE_CART_BIN", str(ROOT / "toolchain/target/debug/continue-cart"))
    with tempfile.TemporaryDirectory(prefix=".catalog-", dir=output.parent) as temporary:
        staging = Path(temporary) / "release"
        staging.mkdir()
        filename = f"{CART_ID}-{version}.cart"
        bundle = staging / filename
        run(binary, "pack", source, "--out", bundle)
        checksum = sha256(bundle)
        verification = json.loads(run(binary, "verify-system-catalog", bundle, "--sha256", checksum))
        require(verification["cart"]["id"] == CART_ID and verification["cart"]["version"] == version, "Cart 身份或版本不符")
        require(checksum == metadata["bundle_sha256"] and bundle.stat().st_size == metadata["bundle_byte_size"], "CI 构建与 Yinks 本地已验证制品不一致")
        require(0 < bundle.stat().st_size <= 8 * 1024 * 1024, "Cart 超出 8 MiB")
        shutil.copyfile(source_root / "release.json", staging / "release.json")
        (staging / "SHA256SUMS").write_text("".join(f"{sha256(path)}  {path.name}\n" for path in [bundle, staging / "release.json"]), encoding="utf-8")
        staging.rename(output)
    return output


def publish(version, output):
    validate_version(version)
    require("-" in version, "尚未配置官方签名，只允许预发布版本")
    repository = os.environ.get("GITHUB_REPOSITORY", "continue-arcade/catalog")
    require(repository == "continue-arcade/catalog", "发布目标必须是 continue-arcade/catalog")
    output = Path(output).resolve()
    tag = "v" + version
    # 修复发布脚本后可在 main 重试原 tag；数据必须仍逐文件等于原 tag，不移动或重建 tag。
    verify_tag_input(version)
    if os.environ.get("GITHUB_SHA"):
        require(run("git", "rev-parse", "HEAD").strip() == os.environ["GITHUB_SHA"], "执行脚本与 workflow 的固定提交不符")
    release = find_release(repository, tag)
    if release is None:
        with tempfile.TemporaryDirectory() as temporary:
            notes = Path(temporary) / "notes.md"
            notes.write_text("Yinks 固定快照生成的系统字典预发布。\n\n范围：类型、平台及正式分类词条；厂商和系列为空。本包未签名，不自动替换应用内置字典。\n\n制品须与 SHA256SUMS 核对。构建输入见本 tag 的 snapshots 目录。\n", encoding="utf-8")
            run("gh", "release", "create", tag, "--repo", repository, "--verify-tag", "--draft", "--prerelease", "--title", f"Continue Catalog {version}", "--notes-file", notes)
        release = find_release(repository, tag)
        require(release is not None, "新建草稿未出现在 Release 列表中，可安全重试")
    release_path = f"repos/{repository}/releases/{release['id']}"
    local_files = {path.name: path for path in output.iterdir() if path.is_file()}
    require(set(local_files) == {f"{CART_ID}-{version}.cart", "release.json", "SHA256SUMS"}, "发布文件集合不符")
    existing = {asset["name"]: asset for asset in release["assets"]}
    require(set(existing).issubset(local_files), "Release 含未知制品，不能改写")
    with tempfile.TemporaryDirectory() as temporary:
        for name, asset in existing.items():
            # gh api 按 asset ID 获取原字节，支持尚未公开的 Draft Release。
            downloaded = subprocess.run(["gh", "api", f"repos/{repository}/releases/assets/{asset['id']}", "-H", "Accept: application/octet-stream"], cwd=ROOT, capture_output=True, timeout=120)
            require(downloaded.returncode == 0 and hashlib.sha256(downloaded.stdout).hexdigest() == sha256(local_files[name]), f"远端同名制品内容不同或读取失败：{name}")
    missing = sorted(set(local_files) - set(existing))
    require(release["draft"] or not missing, "已公开 Release 不能追加或替换内容")
    for name in missing:
        run("gh", "release", "upload", tag, local_files[name], "--repo", repository)
    # 收齐后再次下载验证；上传成功响应不替代整包校验。
    updated = json.loads(run("gh", "api", release_path))
    require({asset["name"] for asset in updated["assets"]} == set(local_files), "Release 尚未收齐制品")
    for asset in updated["assets"]:
        downloaded = subprocess.run(["gh", "api", f"repos/{repository}/releases/assets/{asset['id']}", "-H", "Accept: application/octet-stream"], cwd=ROOT, capture_output=True, timeout=120)
        require(downloaded.returncode == 0 and hashlib.sha256(downloaded.stdout).hexdigest() == sha256(local_files[asset["name"]]), "上传后制品校验失败")
    if updated["draft"]:
        run("gh", "release", "edit", tag, "--repo", repository, "--draft=false", "--prerelease")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["build", "publish", "verify-toolchain"])
    parser.add_argument("version", nargs="?")
    parser.add_argument("--out", default="dist/release")
    arguments = parser.parse_args()
    if arguments.command == "verify-toolchain":
        verify_toolchain()
    elif arguments.command == "build":
        print(build(arguments.version, arguments.out))
    else:
        publish(arguments.version, arguments.out)
