import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

spec = importlib.util.spec_from_file_location("release", Path(__file__).with_name("release.py"))
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def test_version_cannot_escape_source_directory(self):
        for value in ["../1.0.0", "01.0.0", "1.0.0-01", "1.0.0+changed", None]:
            with self.assertRaises(ValueError):
                release.validate_version(value)
        release.validate_version("0.2.0-preview.1")

    def test_exact_toolchain_sources_match_frozen_manifest(self):
        release.verify_toolchain()

    def test_local_pack_matches_fixed_input_and_cannot_overwrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "snapshots/0.2.0-preview.1/source"
            catalog = source / "system/catalog/catalog.json"
            catalog.parent.mkdir(parents=True)
            catalog.write_text("{}")
            (source / "cartfile.toml").write_text("fixture")
            (catalog.parent / "continue.toml").write_text("fixture")
            bundle = b"isolated-test-cart"
            metadata = {"schema": "continue.catalog-release-input.v1", "cart_id": release.CART_ID, "version": "0.2.0-preview.1", "snapshot_sha256": release.sha256(catalog), "bundle_sha256": hashlib.sha256(bundle).hexdigest(), "bundle_byte_size": len(bundle)}
            (source.parent / "release.json").write_text(json.dumps(metadata))

            def run(*args):
                if args[1] == "pack":
                    Path(args[-1]).write_bytes(bundle)
                    return "{}"
                return json.dumps({"cart": {"id": release.CART_ID, "version": metadata["version"]}})

            with patch.object(release, "ROOT", root), patch.object(release, "verify_toolchain"), patch.object(release, "run", side_effect=run):
                output = release.build(metadata["version"], root / "dist")
                self.assertEqual(bundle, (output / f"{release.CART_ID}-{metadata['version']}.cart").read_bytes())
                with self.assertRaises(ValueError):
                    release.build(metadata["version"], output)
                catalog.write_text("tampered")
                with self.assertRaisesRegex(ValueError, "字典摘要"):
                    release.build(metadata["version"], root / "different")
                self.assertFalse((root / "different").exists())

    def test_missing_signing_cannot_publish_stable_release(self):
        with self.assertRaisesRegex(ValueError, "预发布"):
            release.publish("1.0.0", "does-not-exist")

    def test_draft_release_is_found_by_list_and_exact_tag(self):
        draft = {"id": 42, "tag_name": "v0.2.0-preview.1", "draft": True, "assets": []}
        with patch.object(release, "run", return_value=json.dumps([draft])) as request:
            self.assertEqual(draft, release.find_release("continue-arcade/catalog", draft["tag_name"]))
            self.assertIn("/releases?", request.call_args.args[-1])
        with patch.object(release, "run", return_value="[]"):
            self.assertIsNone(release.find_release("continue-arcade/catalog", draft["tag_name"]))

    def test_repair_scripts_cannot_change_original_tag_input(self):
        version = "0.2.0-preview.1"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ["release.json", "source/cartfile.toml", "source/system/catalog/continue.toml", "source/system/catalog/catalog.json"]:
                path = root / "snapshots" / version / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("frozen")
            with patch.object(release, "ROOT", root), patch.object(release, "run", return_value="frozen"):
                release.verify_tag_input(version)
                (root / "snapshots" / version / "release.json").write_text("changed")
                with self.assertRaisesRegex(ValueError, "不可变 tag"):
                    release.verify_tag_input(version)

    def test_new_draft_uses_creation_id_without_waiting_for_list_visibility(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            version = "0.2.0-preview.3"
            files = {f"{release.CART_ID}-{version}.cart": b"cart", "release.json": b"{}", "SHA256SUMS": b"sums"}
            for name, value in files.items():
                (output / name).write_bytes(value)
            assets = [{"id": index, "name": name} for index, name in enumerate(files, 1)]
            calls = []

            def run(*arguments):
                calls.append(arguments)
                if "POST" in arguments and arguments[4] == "repos/continue-arcade/catalog/releases":
                    return json.dumps({"id": 42, "draft": True, "assets": []})
                if arguments == ("gh", "api", "repos/continue-arcade/catalog/releases/42"):
                    return json.dumps({"id": 42, "draft": True, "assets": assets})
                return "{}"

            by_id = {asset["id"]: files[asset["name"]] for asset in assets}

            def download(arguments, **kwargs):
                identifier = int(arguments[2].rsplit("/", 1)[1])
                return SimpleNamespace(returncode=0, stdout=by_id[identifier])

            with patch.dict(release.os.environ, {"GITHUB_REPOSITORY": "continue-arcade/catalog", "GITHUB_SHA": ""}), patch.object(release, "verify_tag_input"), patch.object(release, "find_release", return_value=None) as find, patch.object(release, "run", side_effect=run), patch.object(release.subprocess, "run", side_effect=download):
                release.publish(version, output)
                self.assertEqual(1, find.call_count)
            uploads = [args for args in calls if "POST" in args and "uploads.github.com" in args[4]]
            self.assertEqual(3, len(uploads))
            self.assertTrue(all("/releases/42/assets?" in args[4] for args in uploads))
            self.assertIn(("gh", "api", "--method", "PATCH", "repos/continue-arcade/catalog/releases/42", "-F", "draft=false", "-F", "prerelease=true"), calls)


if __name__ == "__main__":
    unittest.main()
