# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability, please report it responsibly:

1. **Do NOT** open a public GitHub issue.
2. Send a description of the vulnerability to the maintainers via [GitHub Security Advisories](../../security/advisories/new).
3. Include steps to reproduce the issue if possible.
4. We will acknowledge your report within 48 hours.

## Security Best Practices for Deployment

- **JWT Secret**: Always set a strong, random JWT secret in production. Never use the default value.
- **Admin Password**: Change the default admin password immediately after first login.
- **Database Credentials**: Use strong, unique passwords for database connections. Never commit credentials to version control.
- **Network**: Run the application behind a reverse proxy (e.g., Nginx, ALB) with TLS termination.
- **Environment Variables**: Use environment variables or secret managers for all sensitive configuration.
