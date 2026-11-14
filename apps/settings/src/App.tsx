import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  type Config,
  type Language,
  ICON_SIZE,
  MAX_MAGNIFIED_SIZE,
  getAutostart,
  loadConfig,
  saveConfig,
  setAutostart,
} from "./config";
import { Card, Row, Segmented, Slider, Toggle } from "./controls";
import en from "./i18n/en.json";
import es from "./i18n/es.json";

type Section = "dock" | "system";
type Texts = typeof es;

/** Dragging a slider saves at most this often; the dock redraws on each save. */
const SAVE_DELAY_MS = 60;

function textsFor(language: Language): Texts {
  const spanish =
    language === "es" || (language === "auto" && navigator.language.toLowerCase().startsWith("es"));
  return spanish ? es : en;
}

export function App() {
  const [config, setConfig] = useState<Config | null>(null);
  const [autostart, setAutostartState] = useState(false);
  const [section, setSection] = useState<Section>("dock");
  const [failed, setFailed] = useState(false);
  const pending = useRef<Config | null>(null);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    let alive = true;
    Promise.all([loadConfig(), getAutostart()])
      .then(([loaded, starts]) => {
        if (alive) {
          setConfig(loaded);
          setAutostartState(starts);
        }
      })
      .catch(() => alive && setFailed(true));
    return () => {
      alive = false;
      window.clearTimeout(timer.current);
    };
  }, []);

  // Shows the change at once and saves the latest value shortly after, so a
  // slider drag does not write the file on every pixel.
  const update = useCallback((patch: Partial<Config>) => {
    setConfig((current) => {
      if (!current) return current;
      const next = { ...current, ...patch };
      if (next.magnifiedSize < next.iconSize) next.magnifiedSize = next.iconSize;
      pending.current = next;
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => {
        const latest = pending.current;
        pending.current = null;
        if (latest) {
          saveConfig(latest)
            .then(() => setFailed(false))
            .catch(() => setFailed(true));
        }
      }, SAVE_DELAY_MS);
      return next;
    });
  }, []);

  const changeAutostart = useCallback((enabled: boolean) => {
    setAutostartState(enabled);
    setAutostart(enabled).catch(() => {
      setAutostartState(!enabled);
      setFailed(true);
    });
  }, []);

  if (!config) return <div className="window" />;
  const t = textsFor(config.language);
  const appWindow = getCurrentWindow();

  return (
    <div className="window">
      <aside className="sidebar" data-tauri-drag-region>
        <div className="traffic">
          <button
            type="button"
            className="light light-close"
            aria-label={t.close}
            onClick={() => appWindow.close()}
          />
          <button
            type="button"
            className="light light-minimize"
            aria-label={t.minimize}
            onClick={() => appWindow.minimize()}
          />
          <span className="light light-idle" />
        </div>
        <nav>
          <NavItem active={section === "dock"} onSelect={() => setSection("dock")} glyph="▭">
            {t.dock}
          </NavItem>
          <NavItem active={section === "system"} onSelect={() => setSection("system")} glyph="⚙">
            {t.system}
          </NavItem>
        </nav>
      </aside>

      <main className="content">
        <header className="content-title" data-tauri-drag-region>
          {section === "dock" ? t.dock : t.system}
        </header>
        <div className="content-body">
          {section === "dock" ? (
            <>
              <Card>
                <Row label={t.size}>
                  <Slider
                    label={t.size}
                    value={config.iconSize}
                    min={ICON_SIZE.min}
                    max={ICON_SIZE.max}
                    before={t.small}
                    after={t.large}
                    onChange={(iconSize) => update({ iconSize })}
                  />
                </Row>
                <Row label={t.magnification} hint={t.magnificationHint}>
                  <Toggle
                    label={t.magnification}
                    checked={config.magnification}
                    onChange={(magnification) => update({ magnification })}
                  />
                </Row>
                <Row label={t.magnifiedSize}>
                  <Slider
                    label={t.magnifiedSize}
                    value={config.magnifiedSize}
                    min={config.iconSize}
                    max={MAX_MAGNIFIED_SIZE}
                    before={t.small}
                    after={t.large}
                    disabled={!config.magnification}
                    onChange={(magnifiedSize) => update({ magnifiedSize })}
                  />
                </Row>
              </Card>
              <Card>
                <Row label={t.autoHide} hint={t.autoHideHint}>
                  <Toggle
                    label={t.autoHide}
                    checked={config.autoHide}
                    onChange={(autoHide) => update({ autoHide })}
                  />
                </Row>
                <Row label={t.genie} hint={t.genieHint}>
                  <Toggle
                    label={t.genie}
                    checked={config.genie}
                    onChange={(genie) => update({ genie })}
                  />
                </Row>
              </Card>
            </>
          ) : (
            <Card>
              <Row label={t.autostart}>
                <Toggle label={t.autostart} checked={autostart} onChange={changeAutostart} />
              </Row>
              <Row label={t.language}>
                <Segmented<Language>
                  label={t.language}
                  value={config.language}
                  options={[
                    { value: "auto", label: t.languageAuto },
                    { value: "es", label: "Español" },
                    { value: "en", label: "English" },
                  ]}
                  onChange={(language) => update({ language })}
                />
              </Row>
            </Card>
          )}
          {failed ? (
            <p className="error" role="alert">
              {t.saveError}
            </p>
          ) : null}
        </div>
      </main>
    </div>
  );
}

interface NavItemProps {
  active: boolean;
  glyph: string;
  onSelect: () => void;
  children: string;
}

function NavItem({ active, glyph, onSelect, children }: NavItemProps) {
  return (
    <button
      type="button"
      className="nav-item"
      aria-current={active ? "page" : undefined}
      onClick={onSelect}
    >
      <span className="nav-glyph" aria-hidden="true">
        {glyph}
      </span>
      {children}
    </button>
  );
}
