/// Testsprache festnageln: ohne das entscheidet `navigator.language` der
/// Testumgebung über die erwarteten Texte (happy-dom liefert en-US), und die
/// Erwartungen im Test hingen an einem Default, den keiner gesetzt hat.

Object.defineProperty(window.navigator, "language", {
  value: "de",
  configurable: true,
});

/// Node 22 stellt ein globales `localStorage` bereit, das ohne
/// --localstorage-file undefiniert ist und das der Testumgebung überschattet.
/// Ein Speicher im Arbeitsspeicher reicht hier vollauf.
const speicher = new Map<string, string>();
Object.defineProperty(window, "localStorage", {
  configurable: true,
  value: {
    getItem: (k: string) => speicher.get(k) ?? null,
    setItem: (k: string, v: string) => void speicher.set(k, String(v)),
    removeItem: (k: string) => void speicher.delete(k),
    clear: () => speicher.clear(),
    key: (i: number) => [...speicher.keys()][i] ?? null,
    get length() {
      return speicher.size;
    },
  },
});

/// happy-dom kennt keinen ResizeObserver; der ePub-Viewer misst damit seine
/// Fläche. Ein Stub ohne Messungen reicht — die Skalierung fester Seiten
/// hängt an Layoutgrößen, die es in der Testumgebung ohnehin nicht gibt.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
Object.defineProperty(window, "ResizeObserver", {
  configurable: true,
  value: ResizeObserverStub,
});

/// happy-dom kennt kein `document.fonts`; die Tab-Leiste misst nach dem
/// Laden der Schrift das längste Wort. Der Stub ist sofort erfüllt — Breiten
/// gibt es in der Testumgebung ohnehin keine.
Object.defineProperty(document, "fonts", {
  configurable: true,
  value: { ready: Promise.resolve() },
});

/// happy-dom kennt keinen IntersectionObserver; die Archiv-Übersicht lädt
/// damit ihre Bildvorschauen erst beim Sichtbarwerden. Der Stub meldet jedes
/// beobachtete Element sofort als sichtbar — im Test gibt es kein Blickfeld,
/// und geprüft wird, was danach geschieht.
class IntersectionObserverStub {
  ruf: (e: { isIntersecting: boolean; target: Element }[]) => void;
  constructor(ruf: (e: { isIntersecting: boolean; target: Element }[]) => void) {
    this.ruf = ruf;
  }
  observe(el: Element) {
    this.ruf([{ isIntersecting: true, target: el }]);
  }
  unobserve() {}
  disconnect() {}
}
Object.defineProperty(window, "IntersectionObserver", {
  configurable: true,
  value: IntersectionObserverStub,
});
