"""MkDocs hook: turn the knowledge base's [[wiki-links]] into relative Markdown links.

Supported forms (see docs/README.md): [[slug]], [[slug|text]], [[slug#anchor]] and a path-qualified
slug such as [[requirements/README|text]]. A slug is the file name without `.md`; an exact path
relative to docs/ wins over a file name. Without link text, the target's frontmatter `title` is used. Unknown or ambiguous slugs are logged as warnings, so
`mkdocs build --strict` fails on them. Wiki-links inside code spans and fenced code are left alone.
"""

import logging
import posixpath
import re

import yaml

log = logging.getLogger("mkdocs.hooks.wikilinks")

WIKILINK = re.compile(r"\[\[([^\]|#]+)(#[^\]|]*)?(?:\|([^\]]+))?\]\]")
# Fenced code blocks and inline code spans, which must not be rewritten.
CODE = re.compile(r"(^```.*?^```|^~~~.*?^~~~|`[^`\n]+`)", re.MULTILINE | re.DOTALL)
FRONTMATTER = re.compile(r"\A---\n(.*?)\n---\n", re.DOTALL)

_paths: dict[str, str] = {}  # "requirements/README" -> "requirements/README.md"
_names: dict[str, list[str]] = {}  # "README" -> every file with that name
_titles: dict[str, str] = {}


def on_files(files, config):
    _paths.clear()
    _names.clear()
    _titles.clear()
    for f in files.documentation_pages():
        uri = f.src_uri
        stem = uri[: -len(".md")]
        _paths[stem] = uri
        _names.setdefault(posixpath.basename(stem), []).append(uri)
        _titles[uri] = _title(f.abs_src_path) or posixpath.basename(stem)
    return files


def _title(path):
    try:
        with open(path, encoding="utf-8") as fh:
            match = FRONTMATTER.match(fh.read())
        return (yaml.safe_load(match.group(1)) or {}).get("title") if match else None
    except (OSError, yaml.YAMLError):
        return None


def on_page_markdown(markdown, page, config, files):
    source = page.file.src_uri
    # Material shows frontmatter `status` as a page badge. Only outdated docs get one (see extra.status).
    if page.meta.pop("status", None) in ("deprecated", "superseded"):
        page.meta["status"] = "deprecated"

    def link(m):
        slug, anchor, text = m.group(1).strip(), m.group(2) or "", m.group(3)
        candidates = [_paths[slug]] if slug in _paths else _names.get(slug, [])
        if len(candidates) != 1:
            problem = "unknown" if not candidates else f"ambiguous ({', '.join(candidates)})"
            log.warning("%s: wiki-link [[%s]] is %s", source, slug, problem)
            return m.group(0)
        target = candidates[0]
        href = posixpath.relpath(target, posixpath.dirname(source) or ".")
        return f"[{text or _titles[target]}]({href}{anchor})"

    parts = CODE.split(markdown)
    # CODE has one capture group, so odd indexes are code and stay unchanged.
    return "".join(p if i % 2 else WIKILINK.sub(link, p) for i, p in enumerate(parts))
