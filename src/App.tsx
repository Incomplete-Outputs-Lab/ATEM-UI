import { useCallback, useMemo, useState, useSyncExternalStore } from "react";
import type { ConnectionStatus } from "./types";
import {
  autoTransition,
  connectAtem,
  cut,
  disconnectAtem,
  dskAuto,
  setDskOnAir,
  setDskTie,
  setNextTransition,
  setPreviewInputByIndex,
  setProgramInputByIndex,
  toggleAutoBlack
} from "./api/atem";
import { getAtemUiState, refreshAtemState, subscribeAtemUiState } from "./state/atemStore";

const TRANSITIONS = ["mix", "dip", "wipe", "sting", "dve"] as const;
const CAM_ROW_BUTTONS = 10;
const CAM_ROWS = 2;
const CAM_SLOTS = CAM_ROW_BUTTONS * CAM_ROWS;

function statusLabel(status: ConnectionStatus): string {
  if (typeof status === "string") return status;
  if ("Error" in status) return `Error: ${status.Error}`;
  return "Unknown";
}

function sourceIdToToken(sourceId: number): string | null {
  // Exclude direct routing IDs from Program/Preview source buttons.
  if (sourceId >= 11001 && sourceId <= 11999) return null;

  if (sourceId === 0) return "BLK";
  if (sourceId >= 1 && sourceId <= 40) return `CAM${sourceId}`;
  if (sourceId === 1000) return "BARS";
  if (sourceId >= 2001 && sourceId <= 2008) return `COL${sourceId - 2000}`;

  // Media player fill IDs are 3010/3020/3030/3040, key IDs are +1 and excluded.
  if (sourceId >= 3010 && sourceId <= 3041) {
    if (sourceId % 10 === 1) return null;
    return `MP${Math.floor((sourceId - 3000) / 10)}`;
  }

  return null;
}

export default function App() {
  const [ip, setIp] = useState("192.168.1.50");
  const [port, setPort] = useState(9910);
  const [meIndex, setMeIndex] = useState(0);
  const atemUiState = useSyncExternalStore(subscribeAtemUiState, getAtemUiState);
  const snapshot = atemUiState.snapshot;
  const status = atemUiState.status;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [selectedTransition, setSelectedTransition] = useState<(typeof TRANSITIONS)[number]>("mix");

  const dskKey = snapshot.dsk_keys[0] ?? 0;
  const dskRuntime = snapshot.dsk_state[dskKey];
  const dskProps = snapshot.dsk_properties[dskKey];
  const transitionPositionPercent = Math.round(((snapshot.transition_position[meIndex] ?? 0) / 10000) * 100);

  const selectableSources = useMemo(
    () => {
      const base = snapshot.available_source_items
        .map((item) => ({
          sourceId: item.source_id,
          sourceIndex: item.source_index,
          token: sourceIdToToken(item.source_id)
        }))
        .filter((item): item is { sourceId: number; sourceIndex: number; token: string } => item.token !== null)
        .sort((a, b) => {
          const idDiff = a.sourceId - b.sourceId;
          if (idDiff !== 0) return idDiff;
          return a.token.localeCompare(b.token, undefined, { numeric: true });
        });

      // For duplicated tokens, keep the later one.
      // In current device snapshots this maps to the actually controllable source.
      const byToken = new Map<string, { sourceId: number; sourceIndex: number; token: string }>();
      for (const item of base) {
        byToken.set(item.token, item);
      }
      return Array.from(byToken.values()).sort((a, b) => {
        const idDiff = a.sourceId - b.sourceId;
        if (idDiff !== 0) return idDiff;
        return a.token.localeCompare(b.token, undefined, { numeric: true });
      });
    },
    [snapshot.available_source_items]
  );

  const tokenMap = useMemo(() => {
    const map = new Map<string, { sourceId: number; sourceIndex: number; token: string }>();
    selectableSources.forEach((item) => map.set(item.token, item));
    return map;
  }, [selectableSources]);

  const run = useCallback(
    async (work: () => Promise<void>) => {
      setBusy(true);
      setError("");
      try {
        await work();
        await refreshAtemState();
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    },
    []
  );

  const renderTokenButton = (token: string, type: "program" | "preview", key: string) => {
    const mapped = tokenMap.get(token);
    const isActive = mapped
      ? type === "program"
        ? snapshot.program_source_ids[meIndex] === mapped.sourceId
        : snapshot.preview_source_ids[meIndex] === mapped.sourceId
      : false;
    const className = `switch-key ${!mapped ? "disabled-source" : ""} ${
      isActive ? (type === "program" ? "active-red glow-red" : "active-green glow-green") : ""
    }`;
    return (
      <button
        key={key}
        className={className}
        disabled={!mapped}
        onClick={() =>
          run(async () => {
            if (!mapped) return;
            if (type === "program") {
              await setProgramInputByIndex(meIndex, mapped.sourceIndex);
            } else {
              await setPreviewInputByIndex(meIndex, mapped.sourceIndex);
            }
          })
        }
      >
        {token}
      </button>
    );
  };

  const camTokens = useMemo(
    () =>
      selectableSources
        .filter((item) => item.token.startsWith("CAM"))
        .sort((a, b) => {
          const aNum = Number(a.token.replace("CAM", ""));
          const bNum = Number(b.token.replace("CAM", ""));
          return aNum - bNum;
        })
        .map((item) => item.token),
    [selectableSources]
  );

  const camSlots = useMemo(() => {
    const filled = camTokens.slice(0, CAM_SLOTS);
    const padded: (string | null)[] = [...filled];
    while (padded.length < CAM_SLOTS) padded.push(null);
    return padded;
  }, [camTokens]);

  const renderCamRow = (rowIndex: number, type: "program" | "preview") => {
    const start = rowIndex * CAM_ROW_BUTTONS;
    const row = camSlots.slice(start, start + CAM_ROW_BUTTONS);
    return row.map((token, idx) => {
      if (!token) {
        return (
          <button key={`${type}-cam-empty-${rowIndex}-${idx}`} className="switch-key disabled-source" disabled>
            CAM
          </button>
        );
      }
      return (
        renderTokenButton(token, type, `${type}-cam-${rowIndex}-${idx}`)
      );
    });
  };

  return (
    <main className="app">
      <header className="topbar">
        <h1 className="title">ATEM Software Control</h1>
        <div className="connect-strip">
          <input className="field" value={ip} onChange={(e) => setIp(e.target.value)} />
          <input className="field" type="number" value={port} onChange={(e) => setPort(Number(e.target.value))} />
          <button
            className="flat-btn"
            disabled={busy}
            onClick={() =>
              run(async () => {
                await connectAtem(ip, port, true);
              })
            }
          >
            Connect
          </button>
          <button className="flat-btn" disabled={busy} onClick={() => run(async () => disconnectAtem())}>
            Disconnect
          </button>
          <span className="status-tag">{statusLabel(status)}</span>
        </div>
      </header>

      <section className="workspace">
        <section className="switcher-area">
          <div className="switcher-layout">
            <article className="deck-panel grid-panel panel-program">
              <h2>Program</h2>
              <div className="source-layout">
                <div className="cam-grid">
                  <div className="source-grid ten">{renderCamRow(0, "program")}</div>
                  <div className="source-grid ten secondary-gap">{renderCamRow(1, "program")}</div>
                </div>
                <div className="aux-sources">
                  <div className="blk-bars-col">
                    {renderTokenButton("BLK", "program", "program-blk")}
                    {renderTokenButton("BARS", "program", "program-bars")}
                  </div>
                  <div className="color-mp-block">
                    <div className="source-grid two">
                      {renderTokenButton("COL1", "program", "program-col1")}
                      {renderTokenButton("COL2", "program", "program-col2")}
                    </div>
                    <div className="source-grid one secondary-gap">{renderTokenButton("MP1", "program", "program-mp1")}</div>
                  </div>
                </div>
              </div>
            </article>

            <article className="deck-panel grid-panel panel-next-transition">
              <h2>Next Transition</h2>
              <div className="row compact row-nowrap">
                <button className="switch-key disabled-source spacer-btn" disabled aria-hidden="true" />
                <button className="switch-key tiny lit-amber">ON AIR</button>
                <button className="switch-key tiny disabled">ON AIR</button>
                <button className="switch-key tiny disabled">ON AIR</button>
                <button className="switch-key tiny disabled">ON AIR</button>
              </div>
              <div className="row compact">
                <button className="switch-key disabled-source spacer-btn" disabled aria-hidden="true" />
                <button className="switch-key lit-amber">BKGD</button>
                <button className="switch-key">KEY1</button>
                <button className="switch-key disabled">KEY2</button>
                <button className="switch-key disabled">KEY3</button>
                <button className="switch-key disabled">KEY4</button>
              </div>
            </article>

            <article className="deck-panel grid-panel panel-transition-style">
              <h2>Transition Style</h2>
              <div className="row compact">
                {TRANSITIONS.map((transition) => (
                  <button
                    key={transition}
                    className={`switch-key ${transition === "sting" ? "disabled" : ""} ${
                      selectedTransition === transition ? "lit-amber glow-amber" : ""
                    }`}
                    disabled={transition === "sting"}
                    onClick={() =>
                      run(async () => {
                        setSelectedTransition(transition);
                        if (transition !== "sting") {
                          await setNextTransition(meIndex, transition);
                        }
                      })
                    }
                  >
                    {transition.toUpperCase()}
                  </button>
                ))}
              </div>
              <div className="row compact">
                <button className="switch-key prev-trans">PREV TRANS</button>
                <button className="switch-key disabled-source spacer-btn" disabled aria-hidden="true" />
                <button className="switch-key active-red glow-red" onClick={() => run(async () => cut(meIndex))}>
                  CUT
                </button>
                <button
                  className={`switch-key ${snapshot.transition_in_progress[meIndex] ? "lit-amber glow-amber" : ""}`}
                  onClick={() => run(async () => autoTransition(meIndex))}
                >
                  AUTO
                </button>
                <div className="rate-display">0:15</div>
              </div>
            </article>

            <article className="deck-panel grid-panel panel-preview">
              <h2>Preview</h2>
              <div className="source-layout">
                <div className="cam-grid">
                  <div className="source-grid ten">{renderCamRow(0, "preview")}</div>
                  <div className="source-grid ten secondary-gap">{renderCamRow(1, "preview")}</div>
                </div>
                <div className="aux-sources">
                  <div className="blk-bars-col">
                    {renderTokenButton("BLK", "preview", "preview-blk")}
                    {renderTokenButton("BARS", "preview", "preview-bars")}
                  </div>
                  <div className="color-mp-block">
                    <div className="source-grid two">
                      {renderTokenButton("COL1", "preview", "preview-col1")}
                      {renderTokenButton("COL2", "preview", "preview-col2")}
                    </div>
                    <div className="source-grid one secondary-gap">{renderTokenButton("MP1", "preview", "preview-mp1")}</div>
                  </div>
                </div>
              </div>
            </article>

            <article className="tbar-panel deck-panel panel-tbar">
              <h2>T Bar</h2>
              <div className="tbar-shell">
                <div className="tbar-ticks">
                  {Array.from({ length: 20 }).map((_, idx) => (
                    <span key={idx} />
                  ))}
                </div>
                <div className="tbar-track">
                  <input className="tbar-slider" type="range" min={0} max={100} value={transitionPositionPercent} disabled readOnly />
                  <div className="tbar-knob" />
                </div>
              </div>
            </article>

            <article className="deck-panel side-dsk panel-dsk">
              <h2>DSK 1</h2>
              <button
                className={`switch-key ${dskProps?.tie ? "lit-amber glow-amber" : ""}`}
                onClick={() => run(async () => setDskTie(dskKey, !dskProps?.tie))}
              >
                TIE
              </button>
              <div className="rate-display">1:00</div>
              <button
                className={`switch-key ${dskRuntime?.on_air ? "active-red glow-red" : ""}`}
                onClick={() => run(async () => setDskOnAir(dskKey, !dskRuntime?.on_air))}
              >
                ON AIR
              </button>
              <button
                className={`switch-key ${dskRuntime?.in_transition ? "lit-amber glow-amber" : ""}`}
                onClick={() => run(async () => dskAuto(dskKey))}
              >
                AUTO
              </button>
            </article>

            <article className="deck-panel side-ftb panel-ftb">
              <h2>Fade to Black</h2>
              <div className="rate-display">{`${Math.floor((snapshot.ftb_rate[meIndex] ?? 0) / 60)}:${String((snapshot.ftb_rate[meIndex] ?? 0) % 60).padStart(2, "0")}`}</div>
              <button
                className={`switch-key ${
                  snapshot.ftb_fully_black[meIndex] || snapshot.ftb_in_transition[meIndex] ? "active-red glow-red" : ""
                }`}
                onClick={() => run(async () => toggleAutoBlack(meIndex))}
              >
                FTB
              </button>
            </article>
          </div>
        </section>

        <aside className="right-sidebar deck-panel">
          <div className="sidebar-tabs">
            <button className="tab active">Palettes</button>
            <button className="tab">Media</button>
            <button className="tab">HyperDeck</button>
            <button className="tab">Output</button>
          </div>
          <div className="sidebar-list">
            <button className="sidebar-item">Color Generators</button>
            <button className="sidebar-item">Upstream Key 1</button>
            <button className="sidebar-item">Transitions</button>
            <button className="sidebar-item">Downstream Key</button>
            <button className="sidebar-item">Fade to Black</button>
          </div>
        </aside>
      </section>

      <footer className="bottom-nav">
        <button className="nav-btn">⚙</button>
        <button className="nav-btn active">⌘ Switcher</button>
        <button className="nav-btn">▦ Media</button>
        <button className="nav-btn">♪ Audio</button>
        <button className="nav-btn">◉ Camera</button>
        <label className="me-select">
          ME
          <input className="field small" type="number" min={0} value={meIndex} onChange={(e) => setMeIndex(Number(e.target.value))} />
        </label>
      </footer>

      {error && <p className="error-banner">{error}</p>}
    </main>
  );
}
