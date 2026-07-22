/// Markdown-Rendering fürs Panel — eine gemeinsame, entschärfte marked-Instanz
/// für Dokument-Tab und Wiki.
///
/// Warum überhaupt entschärft: marked reicht seit v5 rohes HTML unverändert
/// durch (die alte `sanitize`-Option gibt es nicht mehr), und das Ergebnis geht
/// per innerHTML ins Panel. Das Panel liegt im selben Webview wie das Terminal,
/// dessen `term_write`-Command die PTY über das Fensterlabel adressiert —
/// eingeschleustes Skript könnte also Eingaben in die laufende Shell schreiben.
/// Archiv-Dokumente sind normalerweise die eigenen, aber sobald eines von außen
/// kommt (Git-Sync, geteiltes Verzeichnis, eingefügter Fremdtext), wird aus
/// Markdown-Anzeige Befehlsausführung. Eine CSP gibt es nicht als zweite
/// Schranke (tauri.conf.json: `csp: null`).

import { marked, type Tokens } from "marked";

/// Ziele ohne Schema: Anker, absolute und relative Pfade innerhalb der App.
/// Beide Schrägstrich-Formen müssen den doppelten ausschließen — `//host/x` ist
/// eine schema-relative URL und damit auswärtig. Darum `\/(?!\/)` für den
/// absoluten Pfad und `\.{1,2}\/` für den relativen: Ein `{0,2}` würde den
/// nackten `/` mitmatchen und die erste Regel wirkungslos machen.
const LOCAL = /^(#|\/(?!\/)|\.{1,2}\/)/;

/// Links dürfen auswärts zeigen: Sie brauchen einen Klick, und ein Archiv-
/// Dokument ohne funktionierende Quellenangaben wäre nutzlos.
const OK_LINK = new RegExp(`^(https?:|mailto:)|${LOCAL.source}`, "i");

/// Bilder dagegen nur lokal. Ein `<img>` lädt beim bloßen Anzeigen, ohne jedes
/// Zutun: Ein vergiftetes Archiv-Dokument — der Panel-Inhalt kommt aus einer
/// LLM-Session und ist damit prompt-injizierbar — meldet über die Bild-URL
/// still IP, Zeitpunkt und im Pfad kodierte Daten nach außen.
const OK_IMAGE = LOCAL;

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    // Auch das Apostroph: Sonst hängt die Sicherheit daran, dass jedes Attribut
    // hier doppelt gequotet bleibt — eine Konvention, keine Garantie.
    .replace(/'/g, "&#39;");
}

/// Leeres Ziel statt gefährlichem — der Text bleibt sichtbar, der Klick tut
/// nichts.
function safeHref(href: string, erlaubt: RegExp): string {
  const h = href.trim();
  // Steuerzeichen entfernen: `java\nscript:` ist sonst ein Umgehungsweg.
  const clean = h.replace(/[\u0000-\u001f\u007f]/g, "");
  return erlaubt.test(clean) ? clean : "";
}

const renderer = new marked.Renderer();

// Block- und Inline-HTML laufen beide hier durch: als Text anzeigen, nicht als
// Markup interpretieren.
renderer.html = ({ raw }: Tokens.HTML | Tokens.Tag) => escapeHtml(raw);

renderer.link = function ({ href, title, tokens }: Tokens.Link) {
  const text = this.parser.parseInline(tokens);
  const safe = safeHref(href, OK_LINK);
  const t = title ? ` title="${escapeHtml(title)}"` : "";
  // `rel` auch bei fehlendem target: Der Webview soll dem Ziel keinen Bezug auf
  // das öffnende Fenster und keinen Referrer mitgeben.
  const rel = ` rel="noopener noreferrer"`;
  return safe ? `<a href="${escapeHtml(safe)}"${t}${rel}>${text}</a>` : `<a${t}>${text}</a>`;
};

renderer.image = ({ href, title, text }: Tokens.Image) => {
  const safe = safeHref(href, OK_IMAGE);
  // Auswärtiges Bild: nur der Alt-Text, keine Anfrage nach draußen.
  if (!safe) return escapeHtml(text);
  const t = title ? ` title="${escapeHtml(title)}"` : "";
  return `<img src="${escapeHtml(safe)}" alt="${escapeHtml(text)}"${t}>`;
};

/// Markdown zu HTML, sicher genug für innerHTML.
export function renderMarkdown(src: string): string {
  return marked.parse(src, { async: false, renderer });
}
