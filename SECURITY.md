# Security Policy

## Supported Versions

Only the latest release of Kodo is supported with security updates.

| Version | Supported |
|---------|-----------|
| latest  | Yes       |
| < latest | No       |

## Reporting a Vulnerability

If you discover a security vulnerability in the Kodo compiler, runtime, or
tooling, please report it responsibly.

**Do NOT open a public GitHub issue for security vulnerabilities.**

Instead, use one of the following channels:

1. **GitHub Security Advisories** (preferred): Navigate to the
   [Security tab](https://github.com/rfunix/kodo/security/advisories) of the
   repository and click "Report a vulnerability".

2. **Email**: Send a detailed report to `security@kodo-lang.dev`.

### What to Include

- A description of the vulnerability and its potential impact.
- Steps to reproduce the issue, including sample `.ko` source code if applicable.
- The version of `kodoc` you are using (`kodoc --version`).
- Your operating system and architecture.

### Response Timeline

- **Acknowledgment**: Within 48 hours of receiving your report.
- **Initial assessment**: Within 7 days.
- **Fix or mitigation**: Depending on severity, typically within 30 days.
- **Public disclosure**: Coordinated with the reporter after a fix is available.

## What Counts as a Security Issue

As a compiler project, Kodo has a specific threat model. The following are
considered security issues:

### Critical

- **Code injection via generated binaries**: The compiler produces a native
  binary that executes unintended code not present in the source.
- **Unsafe memory access in the runtime**: The `kodo_runtime` library
  (`libkodo_runtime.a`) dereferences invalid pointers, reads out-of-bounds
  memory, or causes undefined behavior during normal execution.
- **Contract verification bypass**: The contract system (`requires`/`ensures`)
  claims a contract is statically verified when it is not, leading to
  incorrect trust in program safety.

### High

- **Denial of service via compiler input**: A crafted `.ko` source file causes
  the compiler to consume unbounded memory or CPU (e.g., exponential parsing,
  infinite loops in type checking).
- **Information disclosure**: The compiler or runtime leaks sensitive data from
  the build environment into generated binaries (beyond the intended metadata
  embedding).

### Medium

- **SMT solver misuse**: The Z3 integration produces incorrect verification
  results due to encoding errors in the contract-to-SMT translation.
- **Linker command injection**: Crafted module names or file paths lead to
  arbitrary command execution during the linking phase.

### Not Security Issues

The following are bugs, not security vulnerabilities:

- Compiler crashes (panics) on malformed input — these are availability issues
  but not exploitable.
- Incorrect code generation that produces wrong results (correctness bug, not
  security).
- Performance issues in compilation.

## Acknowledgments

We appreciate the security research community's efforts in responsibly
disclosing vulnerabilities. Contributors who report valid security issues will
be credited in the release notes (with their permission).
