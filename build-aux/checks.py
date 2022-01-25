#!/usr/bin/env python3
# Source: https://gitlab.gnome.org/GNOME/fractal/blob/master/hooks/pre-commit.hook
from __future__ import annotations

import os
import subprocess
import sys
import time
from argparse import Namespace
from pathlib import Path
from typing import List, Optional, Tuple
from xml.etree import ElementTree

BOLD_RED = "\033[1;31m"
BOLD_GREEN = "\033[1;32m"
BOLD_YELLOW = "\033[1;33m"
RED = "\033[31m"
GREEN = "\033[32m"
ENDC = "\033[0m"

OK = f"{GREEN}ok{ENDC}"
FAILED = f"{BOLD_RED}FAILED{ENDC}"
SKIPPED = f"{BOLD_YELLOW}SKIPPED{ENDC}"
RUNNING = f"   {BOLD_GREEN}RUNNING{ENDC}"
ERROR = f"{RED}error{ENDC}"


class MissingDependencyError(Exception):
    def __init__(self, whats_missing: str, install_command=None):
        self._whats_missing = whats_missing
        self._install_command = install_command

    def message(self) -> str:
        return f"{ERROR}: Missing dependency `{self._whats_missing}`"

    def suggestion(self) -> str:
        message = f"Please install `{self._whats_missing}` first"

        if self._install_command is not None:
            message += f" by running `{self._install_command}`"

        return message


class FailedCheckError(Exception):
    def __init__(self, error_message=None, suggestion_message=None):
        self._error_message = error_message
        self._suggestion_message = suggestion_message

    def message(self) -> Optional[str]:
        return self._error_message

    def suggestion(self) -> Optional[str]:
        return self._suggestion_message


class Check:
    _prerequisite_checks: List[Check] = []

    def __init__(self, prerequisite_checks: List[Check] = [], skip: bool = False):
        self._prerequisite_checks = prerequisite_checks
        self._skip = skip

    def get_prerequisite_checks(self) -> List[Check]:
        return self._prerequisite_checks

    def get_should_be_skipped(self) -> bool:
        return self._skip

    def version(self) -> Optional[str]:
        return None

    def subject(self) -> str:
        raise NotImplementedError

    def run(self):
        raise NotImplementedError


class Rustfmt(Check):
    """Run rustfmt to enforce code style."""

    def version(self):
        try:
            return get_output(["cargo", "fmt", "--version"])
        except FileNotFoundError:
            return None

    def subject(self):
        return "code style"

    def run(self):
        try:
            return_code, output = run_and_get_output(
                ["cargo", "fmt", "--all", "--", "--check"]
            )
        except FileNotFoundError:
            raise MissingDependencyError(
                "cargo fmt", install_command="rustup component add rustfmt"
            )

        if return_code != 0:
            raise FailedCheckError(
                error_message=output,
                suggestion_message="Try running `cargo fmt --all`",
            )


class Typos(Check):
    """Run typos to check for spelling mistakes."""

    def version(self):
        try:
            return get_output(["typos", "--version"])
        except FileNotFoundError:
            return None

    def subject(self):
        return "spelling mistakes"

    def run(self):
        try:
            return_code, output = run_and_get_output(["typos", "--color", "always"])
        except FileNotFoundError:
            raise MissingDependencyError(
                "typos", install_command="cargo install typos-cli"
            )

        if return_code != 0:
            raise FailedCheckError(
                error_message=output,
                suggestion_message="Try running `typos -w`",
            )


class PotfilesAlphabetically(Check):
    """Check if files in POTFILES are sorted alphabetically.

    This assumes the following:
        - POTFILES is located at 'po/POTFILES.in'
    """

    def subject(self):
        return "po/POTFILES.in alphabetical order"

    def run(self):
        files = self._get_files()

        for file, sorted_file in zip(files, sorted(files)):
            if file != sorted_file:
                raise FailedCheckError(
                    error_message=f"{ERROR}: Found file `{file}` before `{sorted_file}` in POTFILES.in"
                )

    @staticmethod
    def _get_files() -> List[str]:
        with open("po/POTFILES.in") as potfiles_file:
            return [line.strip() for line in potfiles_file.readlines()]


class PotfilesExist(Check):
    """Check if all files in POTFILES exist.

    This assumes the following:
        - POTFILES is located at 'po/POTFILES.in'
    """

    def subject(self):
        return "po/POTFILES.in all files exist"

    def run(self):
        files = self._get_non_existent_files()
        n_files = len(files)

        if n_files > 0:
            message = [
                f"{ERROR}: Found {n_files} file{'s'[:n_files^1]} in POTFILES.in that does not exist:"
            ]

            for file in files:
                message.append(str(file))

            raise FailedCheckError(error_message="\n".join(message))

    @staticmethod
    def _get_non_existent_files() -> List[Path]:
        files = []

        with open("po/POTFILES.in") as potfiles_file:
            for line in potfiles_file.readlines():
                file = Path(line.strip())
                if not file.exists():
                    files.append(file)

        return files


class PotfilesSanity(Check):
    """Check if all files with translatable strings are present and only those.

    The limitations are the following:
        - Only detects UI (Glade) or Rust files

    This assumes the following:
        - POTFILES is located at 'po/POTFILES.in'
        - UI (Glade) files are located in 'data/resources/ui' and use 'translatable="yes"'
        - Rust files are located in 'src' and use '*gettext' methods or macros
    """

    def subject(self):
        return "po/POTFILES.in sanity"

    def run(self):
        potfiles = self._get_rust_or_ui_potfiles()
        files_with_translatable = self._get_ui_files() + self._get_rust_files()

        potfiles_without_translatable = [
            potfile for potfile in potfiles if potfile not in files_with_translatable
        ]
        files_that_should_be_potfile = [
            file for file in files_with_translatable if file not in potfiles
        ]

        n_potfiles_without_translatable = len(potfiles_without_translatable)
        n_files_that_should_be_potfile = len(files_that_should_be_potfile)

        message = []

        if n_potfiles_without_translatable > 0:
            message.append(
                f"{ERROR}: Found {n_potfiles_without_translatable} file{'s'[:n_potfiles_without_translatable^1]} in POTFILES.in without translatable strings:"
            )

            for potfile in sorted(potfiles_without_translatable):
                message.append(str(potfile))

        if n_potfiles_without_translatable > 0 and n_files_that_should_be_potfile > 0:
            message.append("")

        if n_files_that_should_be_potfile > 0:
            message.append(
                f"{ERROR}: Found {n_files_that_should_be_potfile} file{'s'[:n_files_that_should_be_potfile^1]} with translatable strings not present in POTFILES.in:"
            )

            for file in sorted(files_that_should_be_potfile):
                message.append(str(file))

        if n_potfiles_without_translatable > 0 or n_files_that_should_be_potfile > 0:
            raise FailedCheckError(error_message="\n".join(message))

    @staticmethod
    def _get_rust_or_ui_potfiles() -> List[Path]:
        potfiles = []

        with open("po/POTFILES.in") as potfiles_file:
            for line in potfiles_file.readlines():
                file = Path(line.strip())

                if file.suffix in [".ui", ".rs"]:
                    potfiles.append(file)

        return potfiles

    @staticmethod
    def _get_ui_files() -> List[Path]:
        output = get_output(
            "grep -lIr 'translatable=\"yes\"' data/resources/ui/*", shell=True
        )
        return list(map(lambda s: Path(s), output.splitlines()))

    @staticmethod
    def _get_rust_files() -> List[Path]:
        output = get_output(r"grep -lIrE 'gettext[!]?\(' src/*", shell=True)
        return list(map(lambda s: Path(s), output.splitlines()))


class Resources(Check):
    """Check if files in data/resources/resources.gresource.xml are sorted alphabetically.

    This assumes the following:
        - gresource file is located in `data/resources/resources.gresource.xml`
        - only one gresource in the file
    """

    def subject(self):
        return "data/resources/resources.gresource.xml"

    def run(self):
        tree = ElementTree.parse("data/resources/resources.gresource.xml")
        gresource = tree.find("gresource")
        files = [element.text for element in gresource.findall("file")]
        sorted_files = sorted(files, key=lambda f: Path(f).with_suffix(""))

        for file, sorted_file in zip(files, sorted_files):
            if file != sorted_file:
                raise FailedCheckError(
                    error_message=f"{ERROR}: Found file `{file}` before `{sorted_file}` in resources.gresource.xml"
                )


class Runner:
    _checks: List[Check] = []
    _successful_checks: List[Check] = []
    _failed_checks: List[Tuple[Check, Exception]] = []

    def add(self, check: Check):
        self._checks.append(check)

    def run_all(self) -> bool:
        """Returns true if all are successful"""

        n_checks = len(self._checks)

        print(f"{RUNNING} checks at {os.getcwd()}")
        print("")
        print(f"running {n_checks} checks")

        n_skipped = 0
        start_time = time.time()

        for check in self._checks:
            if check.get_should_be_skipped():
                n_skipped += 1
                self._print_result(check, f"{SKIPPED} (via command flag)")
                continue

            if not self._has_complete_prerequisite(check):
                n_skipped += 1
                self._print_has_incomplete_prerequisite(check)
                continue

            try:
                check.run()
            except FailedCheckError as e:
                self._failed_checks.append((check, e))
                self._print_result(check, FAILED)
            except MissingDependencyError as e:
                self._failed_checks.append((check, e))
                self._print_result(check, FAILED)
            else:
                self._successful_checks.append(check)
                self._print_result(check, OK)

        check_duration = time.time() - start_time
        n_successful_checks = len(self._successful_checks)
        n_failed = len(self._failed_checks)

        if n_failed > 0:
            print("")
            self._print_failures()

        print("")
        self._print_final_result(
            n_checks, n_successful_checks, n_failed, n_skipped, check_duration
        )

        return n_failed == 0

    def _has_complete_prerequisite(self, check: Check) -> bool:
        for prerequisite_check in check.get_prerequisite_checks():
            if prerequisite_check not in self._successful_checks:
                return False
        return True

    def _print_has_incomplete_prerequisite(self, check: Check):
        prerequisites_to_print = [
            prerequisite.subject()
            for prerequisite in check.get_prerequisite_checks()
            if prerequisite not in self._successful_checks
        ]

        if len(prerequisites_to_print) == 1:
            requires_message = prerequisites_to_print[0]
        else:
            requires_message = ", ".join(prerequisites_to_print)

        self._print_result(
            check,
            f"{SKIPPED} (requires: {requires_message})",
        )

    def _print_failures(self):
        print("failures:")
        print("")

        for (check, error) in self._failed_checks:
            message = error.message()
            suggestion = error.suggestion()

            if message is not None or suggestion is not None:
                print(f"---- {check.subject()} ----")

            if message is not None:
                print(message)
                print("")

            if suggestion is not None:
                print(suggestion)
                print("")

        print("")
        print("failures:")

        for (check, _) in self._failed_checks:
            print(f"    {check.subject()}")

    @staticmethod
    def _print_result(check: Check, remark: str):
        messages = ["check", check.subject()]

        version = check.version() if args.verbose else None
        if version is not None:
            messages.append(f"({version})")

        messages.append("...")
        messages.append(remark)

        print(" ".join(messages))

    @staticmethod
    def _print_final_result(
        total: int, n_successful: int, n_failed: int, n_skipped: int, duration: float
    ):
        result = OK if n_failed == 0 else FAILED

        print(
            f"test result: {result}. {n_successful} passed; {n_failed} failed; {n_skipped} skipped; finished in {duration:.2f}s"
        )


def run_and_get_output(args: List[str]) -> Tuple[int, str]:
    process = subprocess.run(args, capture_output=True)
    stdout = process.stdout.decode("utf-8").strip()
    stderr = process.stderr.decode("utf-8").strip()
    return (process.returncode, "\n".join([stdout, stderr]).strip())


def get_output(*args, **kwargs) -> str:
    process = subprocess.run(*args, capture_output=True, **kwargs)
    return process.stdout.decode("utf-8").strip()


def main(args: Namespace) -> int:
    runner = Runner()
    runner.add(Rustfmt(skip=args.skip_rustfmt))
    runner.add(Typos(skip=args.skip_typos))

    potfiles_exist = PotfilesExist()
    potfiles_sanity = PotfilesSanity(prerequisite_checks=[potfiles_exist])
    potfiles_alphabetically = PotfilesAlphabetically(
        prerequisite_checks=[potfiles_exist, potfiles_sanity]
    )
    runner.add(potfiles_exist)
    runner.add(potfiles_sanity)
    runner.add(potfiles_alphabetically)

    runner.add(Resources())

    if runner.run_all():
        return os.EX_OK
    else:
        return 1


if __name__ == "__main__":
    from argparse import ArgumentParser

    parser = ArgumentParser(
        description="Run conformity checks on the current Rust project"
    )
    parser.add_argument(
        "-v", "--verbose", action="store_true", help="Use verbose output"
    )
    parser.add_argument(
        "-sr",
        "--skip-rustfmt",
        action="store_true",
        help="Whether to skip running cargo fmt",
    )
    parser.add_argument(
        "-st", "--skip-typos", action="store_true", help="Whether to skip running typos"
    )
    args = parser.parse_args()

    sys.exit(main(args))
