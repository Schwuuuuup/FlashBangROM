Import("env")

from pathlib import Path
import subprocess


def run_git(repo_root, args):
    try:
        result = subprocess.run(
            ["git", "-C", str(repo_root), *args],
            check=True,
            capture_output=True,
            text=True,
        )
        return result.stdout.strip()
    except Exception:
        return ""


def sanitize_tag(tag):
    if not tag:
        return "0.0.0"
    if tag.startswith("v"):
        return tag[1:]
    return tag


project_dir = Path(env.subst("$PROJECT_DIR"))
repo_root = project_dir.parent
generated_header = project_dir / "include" / "generated_build_info.h"
version_file = project_dir / "VERSION"

raw_tag = run_git(repo_root, ["describe", "--tags", "--abbrev=0"])
version_tag = sanitize_tag(raw_tag)
if version_file.exists():
    explicit_version = version_file.read_text(encoding="utf-8").strip()
    if explicit_version:
        version_tag = explicit_version
commit_count = run_git(repo_root, ["rev-list", "--count", "HEAD"]) or "0"
short_sha = run_git(repo_root, ["rev-parse", "--short=8", "HEAD"]) or "nogit"
dirty = 1 if run_git(repo_root, ["status", "--porcelain"]) else 0

version_text = f"{version_tag}+build.{commit_count}.{short_sha}"
if dirty:
    version_text += ".dirty"

generated_header.write_text(
    "#pragma once\n"
    f'static constexpr const char* FB_VERSION_TAG = "{version_tag}";\n'
    f"static constexpr uint32_t FB_BUILD_NUMBER = {commit_count};\n"
    f'static constexpr const char* FB_GIT_SHA = "{short_sha}";\n'
    f"static constexpr bool FB_GIT_DIRTY = {'true' if dirty else 'false'};\n"
    f'static constexpr const char* FB_VERSION_TEXT = "{version_text}";\n'
)
