import { marked } from "marked";
import Prism from "prismjs";
import "prismjs/themes/prism-tomorrow.css";
import "prismjs/plugins/autoloader/prism-autoloader";
import "prismjs/components/prism-javascript";
import "prismjs/components/prism-typescript";
import "prismjs/components/prism-python";

let configured = false;
let highlightEnabled: boolean | null = null;
const GENERATED_CITATION_RE = /\uE200cite((?:\uE202[^\uE200\uE201\uE202]+)+)\uE201/g;
const TRACKING_PARAMS = new Set([
  "fbclid",
  "gclid",
  "igshid",
  "mc_cid",
  "mc_eid",
  "ref",
  "source",
]);

export interface MarkdownSource {
  url: string;
  domain: string;
  label: string;
}

export interface MarkdownSourceLedger {
  sources: MarkdownSource[];
  unresolvedCitations: string[];
}

// Configure autoloader to load other languages on demand
// This reduces initial bundle size while maintaining full language support
if (typeof Prism !== "undefined" && Prism.plugins && Prism.plugins.autoloader) {
  Prism.plugins.autoloader.languages_path =
    "https://cdnjs.cloudflare.com/ajax/libs/prism/1.30.0/components/";
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

function stripHtml(text: string): string {
  return text.replace(/<[^>]*>/g, "");
}

function cleanUrlCandidate(url: string): string {
  return url.trim().replace(/[),.;:!?]+$/g, "");
}

export function normalizeSourceUrl(rawUrl: string): string | null {
  try {
    const url = new URL(cleanUrlCandidate(rawUrl));
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;

    for (const key of Array.from(url.searchParams.keys())) {
      if (key.toLowerCase().startsWith("utm_") || TRACKING_PARAMS.has(key.toLowerCase())) {
        url.searchParams.delete(key);
      }
    }

    return url.toString();
  } catch {
    return null;
  }
}

function normalizeRenderedHref(rawHref: string | null | undefined): string {
  if (!rawHref) return "#";

  const sourceUrl = normalizeSourceUrl(rawHref);
  if (sourceUrl) return sourceUrl;

  const href = rawHref.trim();
  if (
    href.startsWith("#") ||
    href.startsWith("/") ||
    href.startsWith("./") ||
    href.startsWith("../") ||
    href.startsWith("mailto:")
  ) {
    return href;
  }

  return "#";
}

function generatedCitationIds(text: string): string[] {
  const ids: string[] = [];
  text.replace(GENERATED_CITATION_RE, (_match, refs: string) => {
    ids.push(...refs
      .split("\uE202")
      .filter(Boolean)
      .map((ref) => ref.trim())
      .filter(Boolean));
    return "";
  });
  return Array.from(new Set(ids));
}

function stripGeneratedCitations(text: string): string {
  return text
    .replace(GENERATED_CITATION_RE, "")
    .replace(/\s+([,.;:!?])/g, "$1");
}

function sourceLabel(text: string, url: string): string {
  const parsed = new URL(url);
  const cleaned = stripHtml(text)
    .replace(/\s+/g, " ")
    .trim();

  if (cleaned && !/^https?:\/\//i.test(cleaned) && cleaned !== parsed.hostname) {
    return cleaned.length > 90 ? `${cleaned.slice(0, 87)}...` : cleaned;
  }

  return parsed.hostname.replace(/^www\./, "");
}

export function extractMarkdownSources(text: string): MarkdownSourceLedger {
  const sourcesByUrl = new Map<string, MarkdownSource>();
  const addSource = (rawUrl: string, labelText: string) => {
    const url = normalizeSourceUrl(rawUrl);
    if (!url || sourcesByUrl.has(url)) return;

    const parsed = new URL(url);
    sourcesByUrl.set(url, {
      url,
      domain: parsed.hostname.replace(/^www\./, ""),
      label: sourceLabel(labelText, url),
    });
  };

  const markdownLinkRe = /(!?)\[([^\]]+)\]\(([^)\s]+)(?:\s+["'][^)]*["'])?\)/g;
  const textWithoutMarkdownLinks = text.replace(markdownLinkRe, (match, imagePrefix: string, label: string, url: string) => {
    if (!imagePrefix) addSource(url, label);
    return " ";
  });

  const bareUrlRe = /\bhttps?:\/\/[^\s<>"']+/g;
  for (const match of textWithoutMarkdownLinks.matchAll(bareUrlRe)) {
    addSource(match[0], match[0]);
  }

  return {
    sources: Array.from(sourcesByUrl.values()),
    unresolvedCitations: generatedCitationIds(text),
  };
}

function configureMarkdown(enableHighlight: boolean) {
  if (configured && highlightEnabled === enableHighlight) return;

  const renderer = new marked.Renderer();

  renderer.code = ({ text, lang }: { text?: string; lang?: string }) => {
    const code = text || "";
    const language = lang || "text";

    // Prism autoloader will load languages on demand
    // If language isn't loaded yet, fall back to plain text
    let highlightedCode: string;
    try {
      if (enableHighlight && language && Prism.languages[language]) {
        highlightedCode = Prism.highlight(code, Prism.languages[language], language);
      } else {
        highlightedCode = escapeHtml(code);
      }
    } catch (error) {
      console.warn(`Failed to highlight ${language} code:`, error);
      highlightedCode = escapeHtml(code);
    }

    return `
      <div class="code-block-wrapper relative group mb-4">
        <div class="code-block-header">
          <span class="code-language-label">${language}</span>
        </div>
        <button
          class="copy-button opacity-0 group-hover:opacity-100 absolute top-1 right-2
          p-1.5 rounded-md hover:bg-white/10 transition-all duration-200"
          data-copy="${encodeURIComponent(code)}"
        >
          <svg class="w-3.5 h-3.5 text-white/90" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
          </svg>
        </button>
        <pre class="code-block-glass"><code class="language-${language}">${highlightedCode}</code></pre>
      </div>
    `;
  };

  // Make links open in a new window
  renderer.link = function ({ href, title, text }) {
    const safeHref = escapeHtml(normalizeRenderedHref(href));
    const titleAttr = title ? ` title="${escapeHtml(title)}"` : "";
    return `<a href="${safeHref}" target="_blank" rel="noopener noreferrer"${titleAttr}>${text}</a>`;
  };

  marked.setOptions({
    breaks: true,
    gfm: true,
    renderer: renderer,
  });

  configured = true;
  highlightEnabled = enableHighlight;
}

export function renderMarkdown(text: string, options: { enableHighlight: boolean }): string {
  configureMarkdown(options.enableHighlight);
  return marked(stripGeneratedCitations(text));
}
