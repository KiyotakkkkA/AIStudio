# sites

`SiteService` (module root `../sites.rs`; this folder holds its tests): opening web links in
the built-in browser (`sites.open`) and fetching site icons for links, sources and bookmarks
(`sites.icon`). The icon fetcher downloads from arbitrary URLs the model or a page supplied, so
it is built to be safe against server-side request forgery.

## `sites.open`

The URL is trimmed and must be a web URL (`is_web_url`: `http` / `https`); anything else is
`NOT_A_SITE`. It then goes to the shell's page opener, which opens a browser tab.

## `sites.icon` pipeline

1. **Cache.** An in-memory map (cleared when it reaches `ICON_CACHE_MAX`, 512 entries) and a
   disk cache (`DocumentCache` in the icon folder). A cached file whose bytes no longer sniff
   as an image is deleted and refetched.
2. **Candidates** (`icon_candidates`): the icon URL the page declared, if any, then
   `/favicon.ico` on the page's origin.
3. **Fetch** each candidate, following at most `ICON_REDIRECTS` (3) redirects by hand:
   - only `http` and `https`;
   - resolve the host and require **every** resolved address to be public
     (`public_address`); one private address rejects the host;
   - build a client pinned to the checked address (`resolve(domain, address)`), with no
     proxy, no automatic redirects and a 4 s timeout, so DNS cannot be re-resolved to a
     different address between the check and the request (DNS rebinding);
   - read at most `SITE_ICON_BYTES_MAX` bytes;
   - accept only bytes that sniff as PNG, JPEG, GIF, WebP or ICO (`sniff_icon`), regardless
     of the declared content type.
4. Store the result (including "no icon") in the memory cache and found icons on disk.

## What counts as public (`public_address`)

IPv4 rejects loopback, private (10/8, 172.16/12, 192.168/16), link-local, broadcast,
documentation, unspecified, multicast, `0.0.0.0/8`, CGNAT `100.64/10`, `192.0.0.0/24`,
benchmarking `198.18/15` and `240/4`. IPv6 rejects loopback, unspecified, multicast, unique
local `fc00::/7`, link-local `fe80::/10` and documentation `2001:db8::/32`; IPv4-mapped
addresses are checked as IPv4.
