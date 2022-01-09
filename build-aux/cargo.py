#!/usr/bin/env python3

from os import environ, path
from subprocess import run
from argparse import ArgumentParser
from shutil import copy

parser = ArgumentParser()
parser.add_argument("build_root")
parser.add_argument("source_root")
parser.add_argument("output")
parser.add_argument("profile")
parser.add_argument("project_name")
args = parser.parse_args()

environ["CARGO_TARGET_DIR"] = path.join(args.build_root, "target")
environ["CARGO_HOME"] = path.join(args.build_root, "cargo-home")

cargo_toml_path = path.join(args.source_root, "Cargo.toml")

if args.profile == "Devel":
    print("DEBUG MODE")
    run(
        [
            "cargo",
            "build",
            "--manifest-path",
            cargo_toml_path,
        ],
        check=True,
    )
    build_dir = path.join(environ["CARGO_TARGET_DIR"], "debug", args.project_name)
    copy(build_dir, args.output)
else:
    print("RELEASE MODE")
    run(
        [
            "cargo",
            "build",
            "--manifest-path",
            cargo_toml_path,
            "--release",
        ],
        check=True,
    )
    build_dir = path.join(environ["CARGO_TARGET_DIR"], "release", args.project_name)
    copy(build_dir, args.output)
