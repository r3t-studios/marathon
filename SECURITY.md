# Security Policy

## Supported Versions

As an early-stage project (version 0.x.y), security support is limited to the latest development version.

| Version | Supported          |
| ------- | ------------------ |
| mainline branch | :white_check_mark: |
| 0.1.x   | :white_check_mark: |
| < 0.1.0 | :x:                |

## Security Maturity

**Marathon is currently in early development (0.1.x) and is NOT recommended for production use or handling sensitive data.**

Security considerations for the current release:

- ⚠️ **Network protocol** is not hardened against malicious peers
- ⚠️ **Authentication** is not yet implemented
- ⚠️ **Encryption** is provided by QUIC but not verified against attacks
- ⚠️ **Authorization** is not implemented
- ⚠️ **Data validation** is basic and not audited
- ⚠️ **Persistence layer** stores data unencrypted locally

**Use Marathon only in trusted development environments with non-sensitive data.**

## Reporting a Vulnerability

We take security issues seriously. If you discover a security vulnerability in Marathon, please help us address it responsibly.

### How to Report

**Please DO NOT report security vulnerabilities through public GitHub issues.**

Instead, report vulnerabilities by:

1. **Email**: Send details to sienna@linux.com
2. **Subject line**: Include "SECURITY" and a brief description
3. **Include**:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if you have one)

### What to Expect

After you submit a report:

1. **Acknowledgment**: We'll confirm receipt within 48 hours
2. **Assessment**: We'll evaluate the severity and impact within 5 business days
3. **Updates**: We'll keep you informed of our progress
4. **Resolution**: We'll work on a fix and coordinate disclosure timing with you
5. **Credit**: We'll acknowledge your contribution (unless you prefer to remain anonymous)

### Disclosure Timeline

- **Critical vulnerabilities**: Aim to fix within 30 days
- **High severity**: Aim to fix within 60 days
- **Medium/Low severity**: Addressed in regular development cycle

We'll coordinate public disclosure timing with you after a fix is available.

## Security Best Practices

If you're using Marathon (keeping in mind it's not production-ready):

### For Development

- **Use isolated networks** for testing
- **Don't use real user data** or sensitive information
- **Don't expose to the internet** without additional security layers
- **Keep dependencies updated** with `cargo update`
- **Review security advisories** for Rust crates you depend on

### For Deployment (Future)

Once Marathon reaches production readiness, we plan to implement:

- End-to-end encryption for all peer communications
- Peer authentication and authorization
- Encrypted local storage
- Rate limiting and DoS protection
- Security audit trail
- Regular security audits

### Known Security Gaps

Current known limitations (to be addressed before 1.0):

- **No peer authentication** - Any peer can join a session
- **No authorization system** - All peers have full permissions
- **No encrypted storage** - Local SQLite database is unencrypted
- **Limited input validation** - CRDT operations trust peer input
- **No audit logging** - Actions are not logged for security review
- **Network protocol not hardened** - Vulnerable to malicious peers

## Security Contact

For security-related questions or concerns:

- **Email**: sienna@linux.com
- **Response time**: Within 48 hours for initial contact

## Security Advisories

Security advisories will be published:

- In GitHub Security Advisories
- In release notes
- In this SECURITY.md file

Currently, there are no published security advisories.

## Responsible Disclosure

We believe in responsible disclosure and request that you:

- Give us reasonable time to address issues before public disclosure
- Make a good faith effort to avoid privacy violations and service disruption
- Don't exploit vulnerabilities beyond demonstrating the issue
- Don't access or modify data that doesn't belong to you

In return, we commit to:

- Respond promptly to your report
- Keep you informed of our progress
- Credit you for your discovery (if desired)
- Not pursue legal action for good faith security research

## Additional Resources

- [Rust Security Advisory Database](https://rustsec.org/)
- [cargo-audit](https://github.com/RustSec/rustsec/tree/main/cargo-audit) - Audit Rust dependencies
- [OWASP Top 10](https://owasp.org/www-project-top-ten/) - Common web application security risks

## Version History

- **2026-02-06**: Initial security policy for v0.1.0 release

---

**Thank you for helping keep Marathon and its users safe!**
