"""Controlled read-only web research for audit evidence collection.

Fetches public web pages (GitHub repos, official docs) under strict
safety boundaries: HTTPS-only, size limits, timeouts, SSRF protection.
Returns structured source material suitable for evidence creation.

Does NOT install or execute audited projects.
"""

from __future__ import annotations

import ipaddress
import logging
import re
import socket
from dataclasses import dataclass, field
from urllib.parse import urljoin, urlparse

import httpx

logger = logging.getLogger(__name__)

# ── Safety limits ────────────────────────────────────────────────────
MAX_RESPONSE_BYTES = 512 * 1024  # 512 KB
REQUEST_TIMEOUT = 15.0  # seconds
MAX_REDIRECTS = 3
ALLOWED_SCHEMES = frozenset({"https"})
BLOCKED_HOST_SUFFIXES = (
    ".local",
    ".internal",
    ".localhost",
)
BLOCKED_CIDRS = (
    ipaddress.ip_network("10.0.0.0/8"),
    ipaddress.ip_network("127.0.0.0/8"),
    ipaddress.ip_network("169.254.0.0/16"),
    ipaddress.ip_network("172.16.0.0/12"),
    ipaddress.ip_network("192.168.0.0/16"),
    ipaddress.ip_network("::1/128"),
    ipaddress.ip_network("fc00::/7"),
    ipaddress.ip_network("fe80::/10"),
)
USER_AGENT = "AlethOS-ToolPassport/0.1 (+https://github.com/AlethOS/AlethOS-ToolPassport)"


def _is_private_host(host: str) -> bool:
    """Check whether *host* resolves to a private / loopback address."""
    host_lower = host.lower()
    if host_lower in ("localhost", "0.0.0.0", "::1", "[::1]"):
        return True
    if host_lower.endswith(BLOCKED_HOST_SUFFIXES):
        return True
    try:
        ip = ipaddress.ip_address(host)
    except ValueError:
        # Hostname — resolve and check each address.
        try:
            addrs = socket.getaddrinfo(host, None, socket.AF_UNSPEC, socket.SOCK_STREAM)
        except socket.gaierror:
            return True  # cannot resolve → refuse
        for addr_info in addrs:
            sockaddr = addr_info[4]
            ip_str = sockaddr[0]
            try:
                ip = ipaddress.ip_address(ip_str)
            except ValueError:
                continue
            if ip.is_loopback or ip.is_private or ip.is_link_local:
                return True
        return False
    return ip.is_loopback or ip.is_private or ip.is_link_local


def validate_url(url: str) -> None:
    """Raise ValueError if *url* is unsafe to fetch."""
    parsed = urlparse(url)
    if parsed.scheme not in ALLOWED_SCHEMES:
        raise ValueError(f"Only HTTPS URLs are allowed, got: {url}")
    host = (parsed.hostname or "").lower()
    if not host:
        raise ValueError(f"URL has no host: {url}")
    if _is_private_host(host):
        raise ValueError(f"URL resolves to a private/loopback address: {url}")


# ── Response types ───────────────────────────────────────────────────


@dataclass
class SourcePage:
    """A fetched web page ready for evidence extraction."""

    url: str
    status_code: int
    content_type: str = ""
    text: str = ""
    size_bytes: int = 0

    def excerpt(self, max_chars: int = 2_000) -> str:
        """Return the first *max_chars* characters of text content."""
        return self.text[:max_chars]


@dataclass
class ResearchResult:
    """Collected source material for one investigation round."""

    pages: list[SourcePage] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)


# ── Researcher ────────────────────────────────────────────────────────


class Researcher:
    """Fetches public web pages for audit evidence.

    Safety:
    - HTTPS only
    - Blocks private / loopback / link-local addresses (SSRF protection)
    - Caps response at MAX_RESPONSE_BYTES (512 KB)
    - 15-second timeout per request
    - Max 3 redirects, same-host only
    - No cookies, no credentials
    """

    def __init__(self, transport: httpx.BaseTransport | None = None) -> None:
        limits = httpx.Limits(max_connections=4, max_keepalive_connections=2)
        self._client = httpx.Client(
            timeout=REQUEST_TIMEOUT,
            limits=limits,
            follow_redirects=False,
            headers={"User-Agent": USER_AGENT},
            transport=transport,
        )

    def fetch(self, url: str) -> SourcePage:
        """Fetch a single URL and return a SourcePage."""
        initial_host = urlparse(url).hostname
        current_url = url

        for redirect_count in range(MAX_REDIRECTS + 1):
            validate_url(current_url)
            if urlparse(current_url).hostname != initial_host:
                raise ValueError(f"Cross-host redirect is not allowed: {current_url}")

            # Stream response so we can cap total bytes.
            with self._client.stream("GET", current_url) as response:
                if response.is_redirect:
                    location = response.headers.get("location")
                    if not location:
                        raise ValueError(f"Redirect has no location: {current_url}")
                    if redirect_count >= MAX_REDIRECTS:
                        raise ValueError(f"Too many redirects while fetching: {url}")
                    current_url = urljoin(current_url, location)
                    continue

                response.raise_for_status()
                raw = b""
                for chunk in response.iter_bytes(chunk_size=8_192):
                    raw += chunk
                    if len(raw) > MAX_RESPONSE_BYTES:
                        raw = raw[:MAX_RESPONSE_BYTES]
                        logger.warning(
                            "Truncated response from %s at %d bytes",
                            current_url,
                            MAX_RESPONSE_BYTES,
                        )
                        break

                text = raw.decode("utf-8", errors="replace")
                content_type = response.headers.get("content-type", "")

            return SourcePage(
                url=current_url,
                status_code=response.status_code,
                content_type=content_type,
                text=text,
                size_bytes=len(raw),
            )

        raise ValueError(f"Unable to fetch URL: {url}")

    def fetch_github_repo(self, owner: str, repo: str) -> ResearchResult:
        """Fetch the main GitHub repo page and its README.

        Returns a ResearchResult with fetched pages (repo homepage + raw README).
        Errors are captured in result.errors rather than raised.
        """
        result = ResearchResult()
        urls = [
            f"https://github.com/{owner}/{repo}",
            f"https://raw.githubusercontent.com/{owner}/{repo}/main/README.md",
        ]
        for url in urls:
            try:
                page = self.fetch(url)
                result.pages.append(page)
            except (httpx.HTTPError, ValueError, OSError) as exc:
                err_msg = f"Failed to fetch {url}: {exc}"
                logger.warning(err_msg)
                result.errors.append(err_msg)

        return result

    def fetch_urls(self, urls: list[str]) -> ResearchResult:
        """Fetch multiple URLs and collect results."""
        result = ResearchResult()
        for url in urls:
            try:
                page = self.fetch(url)
                result.pages.append(page)
            except (httpx.HTTPError, ValueError, OSError) as exc:
                err_msg = f"Failed to fetch {url}: {exc}"
                logger.warning(err_msg)
                result.errors.append(err_msg)

        return result

    def extract_summary(self, page: SourcePage, max_chars: int = 1_500) -> str:
        """Extract a plain-text summary from a fetched page.

        Strips HTML tags (basic regex) and returns the first *max_chars*
        characters of readable text.
        """
        if "text/html" in page.content_type:
            # Basic HTML-to-text: strip scripts/styles, then tags.
            text = re.sub(
                r"<(script|style)[^>]*>.*?</\1>",
                " ",
                page.text,
                flags=re.DOTALL | re.IGNORECASE,
            )
            text = re.sub(r"<[^>]+>", " ", text)
            text = re.sub(r"\s+", " ", text)
        else:
            text = page.text

        return text.strip()[:max_chars]

    def close(self) -> None:
        """Release the underlying HTTP client."""
        self._client.close()
