import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AtemSnapshotDto, ConnectionStatus, SnapshotEvent } from "./types";

const SOURCE_BUTTONS = 8;

const emptySnapshot: AtemSnapshotDto = {
  initialisation_complete: false,
  mes_count: 1,
  aux_count: 0,
  program_sources: [],
  preview_sources: [],
  available_sources: [],
  tally_by_source: {},
  dsk_keys: [],
  dsk_sources: {},
  dsk_state: {},
  dsk_properties: {},
  transition_position: [],
  transition_in_progress: [],
  ftb_fully_black: [],
  ftb_in_transition: [],
  ftb_frames_remaining: [],
  ftb_rate: []
};

function statusLabel(status: ConnectionStatus): string {
  if (typeof status === "string") return status;
  if ("Error" in status) return `Error: ${status.Error}`;
  return "Unknown";
}

function sourceLabel(source: string, index: number): string {
  const normalized = source.toUpperCase();
  if (normalized.includes("BLACK")) return "BLK";
  if (normalized.includes("COLOR")) return `COL${index + 1}`;
  if (normalized.includes("MEDIA_PLAYER")) return `MP${index + 1}`;
  if (normalized.includes("INPUT")) return `CAM${index + 1}`;
  return source.slice(0, 8);
}

export default function App() {
  const [ip, setIp] = useState("192.168.1.50");
  const [port, setPort] = useState(9910);
  const [meIndex, setMeIndex] = useState(0);
  const [snapshot, setSnapshot] = useState<AtemSnapshotDto>(emptySnapshot);
  const [status, setStatus] = useState<ConnectionStatus>("Disconnected");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const sources = snapshot.available_sources;
  const visibleSources = useMemo(
    () => sources.slice(0, Math.max(SOURCE_BUTTONS, sources.length)),
    [sources]
  );

  const refreshSnapshot = useCallback(async () => {
    const next = await invoke<AtemSnapshotDto>("get_snapshot");
    setSnapshot(next);
    const nextStatus = await invoke<ConnectionStatus>("get_connection_status");
    setStatus(nextStatus);
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    void listen<SnapshotEvent>("atem://snapshot", (event) => {
      setSnapshot(event.payload.snapshot);
      setStatus(event.payload.status);
    }).then((fn) => {
      unlisten = fn;
    });
    void refreshSnapshot();
    return () => {
      if (unlisten) void unlisten();
    };
  }, [refreshSnapshot]);

  const run = useCallback(
    async (work: () => Promise<void>) => {
      setBusy(true);
      setError("");
      try {
        await work();
        await refreshSnapshot();
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    },
    [refreshSnapshot]
  );

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
                await invoke("connect", { ip, port, reconnect: true });
              })
            }
          >
            Connect
          </button>
          <button className="flat-btn" disabled={busy} onClick={() => run(async () => invoke("disconnect"))}>
            Disconnect
          </button>
          <span className="status-tag">{statusLabel(status)}</span>
        </div>
      </header>

      <section className="workspace">
        <div className="left-column">
          <article className="deck-panel">
            <h2>Program</h2>
            <div className="source-grid">
              {visibleSources.map((source, idx) => (
                <button
                  key={`pgm-${idx}`}
                  className={`switch-key ${snapshot.program_sources[meIndex] === source ? "active-red" : ""}`}
                  onClick={() =>
                    run(async () => {
                      await invoke("set_program_input_by_index", { me: meIndex, sourceIndex: idx });
                    })
                  }
                >
                  {sourceLabel(source ?? `SRC${idx + 1}`, idx)}
                </button>
              ))}
            </div>
          </article>

          <article className="deck-panel">
            <h2>Preview</h2>
            <div className="source-grid">
              {visibleSources.map((source, idx) => (
                <button
                  key={`pvw-${idx}`}
                  className={`switch-key ${snapshot.preview_sources[meIndex] === source ? "active-green" : ""}`}
                  onClick={() =>
                    run(async () => {
                      await invoke("set_preview_input_by_index", { me: meIndex, sourceIndex: idx });
                    })
                  }
                >
                  {sourceLabel(source ?? `SRC${idx + 1}`, idx)}
                </button>
              ))}
            </div>
          </article>
        </div>

        <article className="center-column deck-panel">
          <h2>Transition</h2>
          <div className="transition-block">
            <label className="me-select">
              ME
              <input className="field small" type="number" min={0} value={meIndex} onChange={(e) => setMeIndex(Number(e.target.value))} />
            </label>
            <div className="row">
              <button className="switch-key wide" onClick={() => run(async () => invoke("cut", { me: meIndex }))}>
                CUT
              </button>
              <button className="switch-key wide" onClick={() => run(async () => invoke("auto_transition", { me: meIndex }))}>
                AUTO
              </button>
            </div>
            <div className="row">
              <button className="switch-key" onClick={() => run(async () => invoke("set_next_transition", { me: meIndex, transition: "mix" }))}>
                MIX
              </button>
              <button className="switch-key" onClick={() => run(async () => invoke("set_next_transition", { me: meIndex, transition: "dip" }))}>
                DIP
              </button>
              <button className="switch-key" onClick={() => run(async () => invoke("set_next_transition", { me: meIndex, transition: "wipe" }))}>
                WIPE
              </button>
            </div>
          </div>
        </article>

        <article className="right-column deck-panel">
          <h2>Downstream Key</h2>
          <p className="meta-line">Keys: {snapshot.dsk_keys.join(", ") || "none"}</p>
          <p className="meta-line">Rate: {snapshot.ftb_rate[meIndex] ?? 0}</p>
          <p className="meta-line">In Transition: {snapshot.transition_in_progress[meIndex] ? "Yes" : "No"}</p>
          <div className="status-dot-row">
            <span className={snapshot.ftb_fully_black[meIndex] ? "dot on" : "dot"} />
            <span>Fade To Black</span>
          </div>
        </article>
      </section>
      {error && <p className="error-banner">{error}</p>}
    </main>
  );
}
