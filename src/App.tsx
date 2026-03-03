import { useEffect, useState, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize, PhysicalPosition } from "@tauri-apps/api/dpi";
import "./App.css";

const SIZE_PICKER = new LogicalSize(320, 440);
const SIZE_DETAIL = new LogicalSize(250, 500);
const SIZE_DETAIL_MINI = new LogicalSize(64, 350);
const SIZE_SETTINGS = new LogicalSize(340, 500);
const SIZE_BUFF_FORM = new LogicalSize(340, 360);

function startDrag(e: React.MouseEvent) {
  e.preventDefault();
  getCurrentWindow().startDragging();
}

/** Resize window while keeping the right edge anchored. */
async function resizeRightAnchored(newSize: LogicalSize) {
  const win = getCurrentWindow();
  const oldSize = await win.outerSize();   // PhysicalSize
  const pos = await win.outerPosition();   // PhysicalPosition
  const factor = await win.scaleFactor();

  // Current right edge in physical pixels
  const rightEdge = pos.x + oldSize.width;
  const newPhysicalW = Math.round(newSize.width * factor);

  // Resize first (macOS may reposition the window during setSize)
  await win.setSize(newSize);
  // Then override position so the right edge stays put
  await win.setPosition(new PhysicalPosition(rightEdge - newPhysicalW, pos.y));
}

interface TimerDef {
  id: string;
  name: string;
  icon: string;
  duration_secs: number;
  hotkey: string | null;
  color: string;
  warning_secs: number;
  repeat: boolean;
  description: string | null;
}

interface BossConfig {
  boss: { name: string; description: string };
  timers: TimerDef[];
}

interface Timer {
  id: string;
  name: string;
  icon: string;
  duration: number;
  remaining: number;
  state: "Running" | "Warning" | "Expired";
  color: string;
  warning_secs: number;
  def_id: string;
  timer_type: string;
}

interface TimerUpdate {
  timers: Timer[];
}

interface BossListItem {
  id: string;
  name: string;
  description: string;
  timer_count: number;
}

interface AppSettings {
  back_hotkey: string;
  stop_all_hotkey: string;
  hotkeys: Record<string, Record<string, string>>;
  hidden_timers: Record<string, string[]>;
  mini_mode: boolean;
}

interface BossHotkeyInfo {
  timer_id: string;
  timer_name: string;
  default_hotkey: string | null;
  effective_hotkey: string | null;
}

interface SelectBossResponse {
  config: BossConfig;
  hidden_timers: string[];
  muted_timers: string[];
  mini_mode: boolean;
}

interface BuffItem {
  id: string;
  name: string;
  duration_secs: number;
  hotkey: string | null;
  enabled: boolean;
}

// --- MechanicRow: full-size mechanic display ---
function MechanicRow({
  def,
  timer,
  effectiveHotkey,
  isMuted,
  onHide,
  onToggleMute,
  miniMode,
}: {
  def: TimerDef;
  timer: Timer | undefined;
  effectiveHotkey: string | null;
  isMuted: boolean;
  onHide: () => void;
  onToggleMute: () => void;
  miniMode: boolean;
}) {
  const [showDesc, setShowDesc] = useState(false);
  const isActive = !!timer;
  const remaining = timer ? Math.max(0, timer.remaining) : def.duration_secs;
  const displayTime = remaining < 1 && timer ? remaining.toFixed(1) : `${Math.ceil(remaining)}`;
  const progress = def.duration_secs > 0 ? remaining / def.duration_secs : 0;

  const stateClass = timer
    ? timer.state === "Expired"
      ? "mechanic-expired"
      : timer.state === "Warning"
        ? "mechanic-warning"
        : "mechanic-active"
    : "mechanic-idle";

  return (
    <div className={`mechanic-row ${stateClass}`}>
      <span className="mechanic-icon">{def.icon}</span>
      <div className="mechanic-body">
        <div className="mechanic-header">
          <span className="mechanic-name">{def.name}</span>
          <div className="mechanic-right">
            {!miniMode && def.description && (
              <button
                className="mechanic-info-btn"
                onClick={() => setShowDesc((v) => !v)}
                title="說明 / Info"
              >
                {showDesc ? "▴" : "▾"}
              </button>
            )}
            {effectiveHotkey && (
              <span className="mechanic-hotkey">{effectiveHotkey}</span>
            )}
            <button
              className={`mechanic-mute-btn ${isMuted ? "mechanic-muted" : ""}`}
              onClick={onToggleMute}
              title={isMuted ? "取消靜音 / Unmute" : "靜音 / Mute"}
            >
              {isMuted ? "🔇" : "🔊"}
            </button>
            {!miniMode && (
              <button className="mechanic-hide-btn" onClick={onHide}>
                ✕
              </button>
            )}
          </div>
        </div>
        {showDesc && def.description && (
          <span className="mechanic-desc">{def.description}</span>
        )}
        <div className="mechanic-bar-bg">
          <div
            className="mechanic-bar-fill"
            style={{
              width: `${progress * 100}%`,
              backgroundColor: isActive ? def.color : "rgba(255,255,255,0.15)",
            }}
          />
        </div>
        <span className="mechanic-time">
          {timer?.state === "Expired" ? "TIME!" : `${displayTime}s`}
        </span>
      </div>
    </div>
  );
}

// --- MiniTile: 32x32 buff icon like MapleStory buff bar ---
function MiniTile({
  def,
  timer,
}: {
  def: TimerDef;
  timer: Timer | undefined;
}) {
  const remaining = timer ? Math.max(0, timer.remaining) : def.duration_secs;
  const displayTime = remaining < 1 && timer ? remaining.toFixed(1) : `${Math.ceil(remaining)}`;
  const progress = def.duration_secs > 0 ? remaining / def.duration_secs : 1;
  // Clockwise sweep: dark covers elapsed time, clear = remaining
  const elapsedAngle = (1 - progress) * 360;

  const stateClass = timer
    ? timer.state === "Expired"
      ? "buff-expired"
      : timer.state === "Warning"
        ? "buff-warning"
        : "buff-active"
    : "buff-idle";

  return (
    <div className={`buff-icon ${stateClass}`}>
      <span className="buff-emoji">{def.icon}</span>
      <span className="buff-secs">
        {timer?.state === "Expired" ? "!" : displayTime}
      </span>
      {timer && timer.state !== "Expired" && (
        <div
          className="buff-sweep"
          style={{
            background: `conic-gradient(rgba(0,0,0,0.6) ${elapsedAngle}deg, transparent ${elapsedAngle}deg)`,
          }}
        />
      )}
    </div>
  );
}

// --- BossDetailPage ---
function BossDetailPage({
  config,
  timers,
  hiddenTimers,
  mutedTimers,
  miniMode,
  hotkeys,
  onBack,
  onHideTimer,
  onToggleMute,
  onResetHidden,
  onToggleMini,
}: {
  config: BossConfig;
  timers: Timer[];
  hiddenTimers: string[];
  mutedTimers: string[];
  miniMode: boolean;
  hotkeys: Record<string, string>;
  onBack: () => void;
  onHideTimer: (timerId: string) => void;
  onToggleMute: (timerId: string) => void;
  onResetHidden: () => void;
  onToggleMini: () => void;
}) {
  const visibleDefs = config.timers.filter(
    (d) => !hiddenTimers.includes(d.id)
  );

  // Map running timers to their defs (prefer non-expired over expired)
  const timerByDef: Record<string, Timer> = {};
  for (const t of timers) {
    const existing = timerByDef[t.def_id];
    if (!existing || (existing.state === "Expired" && t.state !== "Expired")) {
      timerByDef[t.def_id] = t;
    }
  }

  // --- Mini mode: single-column buff bar ---
  if (miniMode) {
    return (
      <div className="buff-bar">
        <div className="buff-bar-drag" onMouseDown={startDrag}>
          <span className="buff-bar-grip">⋮</span>
        </div>
        {visibleDefs.map((def) => (
          <MiniTile key={def.id} def={def} timer={timerByDef[def.id]} />
        ))}
        <div className="buff-bar-actions">
          <button
            className="buff-bar-btn"
            onClick={onToggleMini}
            onMouseDown={(e) => e.stopPropagation()}
            title="展開 / Expand"
          >
            ⊟
          </button>
        </div>
      </div>
    );
  }

  // --- Normal mode: transparent HUD overlay ---
  return (
    <div className="hud-detail">
      <div className="hud-header" onMouseDown={startDrag}>
        <span className="hud-boss-name">{config.boss.name}</span>
        <div className="hud-actions">
          {hiddenTimers.length > 0 && (
            <button
              className="hud-btn"
              onClick={onResetHidden}
              onMouseDown={(e) => e.stopPropagation()}
              title="重置隱藏 / Reset Hidden"
            >
              ↺
            </button>
          )}
          <button
            className="hud-btn"
            onClick={onToggleMini}
            onMouseDown={(e) => e.stopPropagation()}
            title="迷你模式 / Mini Mode"
          >
            ⊞
          </button>
          <button
            className="hud-btn hud-btn-close"
            onClick={onBack}
            onMouseDown={(e) => e.stopPropagation()}
          >
            ✕
          </button>
        </div>
      </div>
      <div className="mechanic-list">
        {visibleDefs.map((def) => (
          <MechanicRow
            key={def.id}
            def={def}
            timer={timerByDef[def.id]}
            effectiveHotkey={hotkeys[def.id] || def.hotkey}
            isMuted={mutedTimers.includes(def.id)}
            onHide={() => onHideTimer(def.id)}
            onToggleMute={() => onToggleMute(def.id)}
            miniMode={miniMode}
          />
        ))}
      </div>
    </div>
  );
}

// --- HotkeyCapture ---
function buildHotkeyString(e: React.KeyboardEvent): string | null {
  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("Ctrl");
  if (e.shiftKey) parts.push("Shift");
  if (e.altKey) parts.push("Alt");

  const key = e.key;
  if (["Control", "Shift", "Alt", "Meta"].includes(key)) {
    return null;
  }

  const keyMap: Record<string, string> = {
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    " ": "Space",
    "`": "Backquote",
  };

  const mappedKey = keyMap[key] || (key.length === 1 ? key.toUpperCase() : key);
  parts.push(mappedKey);

  return parts.join("+");
}

function HotkeyCapture({
  currentHotkey,
  onCapture,
  onCancel,
}: {
  currentHotkey: string | null;
  onCapture: (hotkey: string) => void;
  onCancel: () => void;
}) {
  const [capturing, setCapturing] = useState(false);
  const captureRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (capturing && captureRef.current) {
      captureRef.current.focus();
    }
  }, [capturing]);

  if (!capturing) {
    return (
      <span className="hotkey-display" onClick={() => setCapturing(true)}>
        {currentHotkey || "未設定"}
      </span>
    );
  }

  return (
    <span
      ref={captureRef}
      className="hotkey-capture"
      tabIndex={0}
      onKeyDown={(e) => {
        e.preventDefault();
        e.stopPropagation();
        if (e.key === "Escape") {
          setCapturing(false);
          onCancel();
          return;
        }
        const hotkey = buildHotkeyString(e);
        if (hotkey) {
          setCapturing(false);
          onCapture(hotkey);
        }
      }}
      onBlur={() => {
        setCapturing(false);
        onCancel();
      }}
    >
      按下快捷鍵...
    </span>
  );
}

// --- SettingsPage ---
function SettingsPage({
  bosses,
  onBack,
}: {
  bosses: BossListItem[];
  onBack: () => void;
}) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [bossHotkeys, setBossHotkeys] = useState<Record<string, BossHotkeyInfo[]>>({});
  useEffect(() => {
    invoke<AppSettings>("get_settings").then(setSettings);
    Promise.all(
      bosses.map((b) =>
        invoke<BossHotkeyInfo[]>("get_boss_hotkeys", { bossId: b.id }).then(
          (hotkeys) => ({ bossId: b.id, hotkeys })
        )
      )
    ).then((results) => {
      const map: Record<string, BossHotkeyInfo[]> = {};
      for (const r of results) {
        map[r.bossId] = r.hotkeys;
      }
      setBossHotkeys(map);
    });
  }, [bosses]);

  const persistSettings = async (newSettings: AppSettings) => {
    setSettings(newSettings);
    await invoke("save_settings", { payload: { settings: newSettings } });
  };

  const updateGlobalHotkey = (field: "back_hotkey" | "stop_all_hotkey", value: string) => {
    if (!settings) return;
    persistSettings({ ...settings, [field]: value });
  };

  const updateTimerHotkey = (bossId: string, timerId: string, hotkey: string) => {
    if (!settings) return;
    const newHotkeys = { ...settings.hotkeys };
    if (!newHotkeys[bossId]) {
      newHotkeys[bossId] = {};
    }
    newHotkeys[bossId] = { ...newHotkeys[bossId], [timerId]: hotkey };
    persistSettings({ ...settings, hotkeys: newHotkeys });

    const newBossHotkeys = { ...bossHotkeys };
    if (newBossHotkeys[bossId]) {
      newBossHotkeys[bossId] = newBossHotkeys[bossId].map((h) =>
        h.timer_id === timerId ? { ...h, effective_hotkey: hotkey } : h
      );
      setBossHotkeys(newBossHotkeys);
    }
  };

  const resetTimerHotkey = (bossId: string, timerId: string, defaultHotkey: string | null) => {
    if (!settings) return;
    const newHotkeys = { ...settings.hotkeys };
    if (newHotkeys[bossId]) {
      const { [timerId]: _, ...rest } = newHotkeys[bossId];
      if (Object.keys(rest).length === 0) {
        delete newHotkeys[bossId];
      } else {
        newHotkeys[bossId] = rest;
      }
    }
    persistSettings({ ...settings, hotkeys: newHotkeys });

    const newBossHotkeys = { ...bossHotkeys };
    if (newBossHotkeys[bossId]) {
      newBossHotkeys[bossId] = newBossHotkeys[bossId].map((h) =>
        h.timer_id === timerId ? { ...h, effective_hotkey: defaultHotkey } : h
      );
      setBossHotkeys(newBossHotkeys);
    }
  };

  if (!settings) return null;

  return (
    <div className="settings-page">
      <div className="window-header" onMouseDown={startDrag}>
        <div className="picker-title">快捷鍵設定</div>
        <button className="close-btn" onClick={() => getCurrentWindow().close()} onMouseDown={(e) => e.stopPropagation()}>
          ✕
        </button>
      </div>

      <div className="settings-scroll">
        <div className="picker-subtitle">Hotkey Settings</div>

        <div className="settings-section">
          <div className="settings-section-title">全局 / Global</div>
          <div className="hotkey-row">
            <span className="hotkey-label">返回主畫面</span>
            <div className="hotkey-right">
              <HotkeyCapture
                currentHotkey={settings.back_hotkey}
                onCapture={(hk) => updateGlobalHotkey("back_hotkey", hk)}
                onCancel={() => {}}
              />
              {settings.back_hotkey !== "Alt+Home" && (
                <button
                  className="hotkey-reset-btn"
                  onClick={() => updateGlobalHotkey("back_hotkey", "Alt+Home")}
                  title="重置 / Reset"
                >
                  ↺
                </button>
              )}
            </div>
          </div>
          <div className="hotkey-row">
            <span className="hotkey-label">停止全部</span>
            <div className="hotkey-right">
              <HotkeyCapture
                currentHotkey={settings.stop_all_hotkey}
                onCapture={(hk) => updateGlobalHotkey("stop_all_hotkey", hk)}
                onCancel={() => {}}
              />
              {settings.stop_all_hotkey !== "Ctrl+0" && (
                <button
                  className="hotkey-reset-btn"
                  onClick={() => updateGlobalHotkey("stop_all_hotkey", "Ctrl+0")}
                  title="重置 / Reset"
                >
                  ↺
                </button>
              )}
            </div>
          </div>
        </div>

        {bosses.map((boss) => (
          <div key={boss.id} className="settings-section">
            <div className="settings-section-title">{boss.name}</div>
            {(bossHotkeys[boss.id] || []).map((hk) => (
              <div key={hk.timer_id} className="hotkey-row">
                <span className="hotkey-label">{hk.timer_name}</span>
                <div className="hotkey-right">
                  <HotkeyCapture
                    currentHotkey={hk.effective_hotkey}
                    onCapture={(hotkey) => updateTimerHotkey(boss.id, hk.timer_id, hotkey)}
                    onCancel={() => {}}
                  />
                  {hk.effective_hotkey !== hk.default_hotkey && (
                    <button
                      className="hotkey-reset-btn"
                      onClick={() => resetTimerHotkey(boss.id, hk.timer_id, hk.default_hotkey)}
                      title="重置 / Reset"
                    >
                      ↺
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        ))}
      </div>

      <div className="settings-footer">
        <button className="settings-btn back-btn" onClick={onBack}>
          返回 / Back
        </button>
      </div>
    </div>
  );
}

// --- BuffTabContent: inline buff list for homepage tab ---
function BuffTabContent({
  onAdd,
  onEdit,
}: {
  onAdd: () => void;
  onEdit: (buff: BuffItem) => void;
}) {
  const [buffs, setBuffs] = useState<BuffItem[]>([]);
  const [deleteConfirm, setDeleteConfirm] = useState<string | null>(null);

  const loadBuffs = useCallback(async () => {
    const list = await invoke<BuffItem[]>("list_buffs");
    setBuffs(list);
  }, []);

  useEffect(() => {
    loadBuffs();
  }, [loadBuffs]);

  const handleToggleEnabled = async (buff: BuffItem) => {
    await invoke<BuffItem>("update_buff", {
      payload: { id: buff.id, enabled: !buff.enabled },
    });
    loadBuffs();
  };

  const handleDelete = async (buffId: string) => {
    await invoke("delete_buff", { buffId });
    setDeleteConfirm(null);
    loadBuffs();
  };

  return (
    <div className="home-tab-content">
      {buffs.length === 0 ? (
        <div className="buff-empty-state">
          尚無 Buff 提醒，點擊下方按鈕新增
        </div>
      ) : (
        <div className="buff-list">
          {buffs.map((buff) => (
            <div
              key={buff.id}
              className={`buff-card ${!buff.enabled ? "buff-card-disabled" : ""}`}
            >
              <div className="buff-card-main">
                <div className="buff-card-info">
                  <span className="buff-card-name">{buff.name}</span>
                  <span className="buff-card-detail">
                    {buff.duration_secs}s
                    {buff.hotkey && (
                      <span className="buff-card-hotkey">{buff.hotkey}</span>
                    )}
                  </span>
                </div>
                <div className="buff-card-actions">
                  <label className="buff-toggle">
                    <input
                      type="checkbox"
                      checked={buff.enabled}
                      onChange={() => handleToggleEnabled(buff)}
                    />
                    <span className="buff-toggle-slider" />
                  </label>
                  <button
                    className="buff-card-btn"
                    onClick={() => onEdit(buff)}
                    title="編輯"
                  >
                    ✎
                  </button>
                  {deleteConfirm === buff.id ? (
                    <div className="buff-delete-confirm">
                      <button
                        className="buff-card-btn buff-card-btn-danger"
                        onClick={() => handleDelete(buff.id)}
                      >
                        確定
                      </button>
                      <button
                        className="buff-card-btn"
                        onClick={() => setDeleteConfirm(null)}
                      >
                        取消
                      </button>
                    </div>
                  ) : (
                    <button
                      className="buff-card-btn buff-card-btn-danger"
                      onClick={() => setDeleteConfirm(buff.id)}
                      title="刪除"
                    >
                      ✕
                    </button>
                  )}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
      <div className="buff-tab-actions">
        <button className="settings-btn buff-add-btn" onClick={onAdd}>
          + 新增 Buff
        </button>
      </div>
    </div>
  );
}

// --- BuffFormPage ---
function BuffFormPage({
  editBuff,
  onSave,
  onCancel,
}: {
  editBuff: BuffItem | null;
  onSave: () => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(editBuff?.name ?? "");
  const [durationSecs, setDurationSecs] = useState(
    editBuff?.duration_secs?.toString() ?? ""
  );
  const [hotkey, setHotkey] = useState<string | null>(editBuff?.hotkey ?? null);
  const [error, setError] = useState<string | null>(null);

  const handleSave = async () => {
    const trimmedName = name.trim();
    if (!trimmedName) {
      setError("請輸入 Buff 名稱");
      return;
    }
    const secs = parseInt(durationSecs, 10);
    if (!secs || secs <= 0) {
      setError("秒數必須為正整數");
      return;
    }
    setError(null);

    if (editBuff) {
      await invoke("update_buff", {
        payload: {
          id: editBuff.id,
          name: trimmedName,
          duration_secs: secs,
          hotkey: hotkey || null,
        },
      });
    } else {
      await invoke("add_buff", {
        payload: {
          name: trimmedName,
          duration_secs: secs,
          hotkey: hotkey || null,
        },
      });
    }
    onSave();
  };

  return (
    <div className="settings-page">
      <div className="window-header" onMouseDown={startDrag}>
        <div className="picker-title">
          {editBuff ? "編輯 Buff" : "新增 Buff"}
        </div>
        <button
          className="close-btn"
          onClick={() => getCurrentWindow().close()}
          onMouseDown={(e) => e.stopPropagation()}
        >
          ✕
        </button>
      </div>

      <div className="settings-scroll">
        <div className="buff-form">
          <div className="buff-form-field">
            <label className="buff-form-label">名稱 / Name</label>
            <input
              className="buff-form-input"
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="例如：聖魂劍士"
              autoFocus
            />
          </div>

          <div className="buff-form-field">
            <label className="buff-form-label">秒數 / Duration (sec)</label>
            <input
              className="buff-form-input"
              type="number"
              min="1"
              value={durationSecs}
              onChange={(e) => setDurationSecs(e.target.value)}
              placeholder="例如：180"
            />
          </div>

          <div className="buff-form-field">
            <label className="buff-form-label">快捷鍵 / Hotkey</label>
            <HotkeyCapture
              currentHotkey={hotkey}
              onCapture={(hk) => setHotkey(hk)}
              onCancel={() => {}}
            />
            {hotkey && (
              <button
                className="hotkey-reset-btn"
                onClick={() => setHotkey(null)}
                title="清除快捷鍵"
                style={{ marginLeft: 6 }}
              >
                ↺
              </button>
            )}
          </div>

          {error && <div className="buff-form-error">{error}</div>}
        </div>
      </div>

      <div className="settings-footer buff-footer">
        <button className="settings-btn buff-save-btn" onClick={handleSave}>
          儲存 / Save
        </button>
        <button className="settings-btn back-btn" onClick={onCancel}>
          取消 / Cancel
        </button>
      </div>
    </div>
  );
}

// --- BuffHudApp: standalone window for buff timers ---
function BuffHudApp() {
  const [timers, setTimers] = useState<Timer[]>([]);
  const prevCountRef = useRef(0);

  useEffect(() => {
    const unlisten = listen<TimerUpdate>("timer-update", (event) => {
      setTimers(event.payload.timers);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const buffTimers = timers.filter((t) => t.timer_type === "buff");

  // Show/hide and resize window based on active buff timers
  useEffect(() => {
    const win = getCurrentWindow();
    if (buffTimers.length > 0) {
      // 14px drag handle + 3px gap, then each icon 48px + 3px gap
      const height = 14 + 3 + buffTimers.length * 51;
      win.setSize(new LogicalSize(64, height));
      win.show();
    } else if (prevCountRef.current > 0) {
      win.hide();
    }
    prevCountRef.current = buffTimers.length;
  }, [buffTimers.length]);

  if (buffTimers.length === 0) return null;

  return (
    <div className="buff-hud">
      <div className="buff-bar-drag" onMouseDown={startDrag}>
        <span className="buff-bar-grip">⋮</span>
      </div>
      {buffTimers.map((timer) => {
        const rem = Math.max(0, timer.remaining);
        const displayTime = rem < 1 ? rem.toFixed(1) : `${Math.ceil(rem)}`;
        const progress =
          timer.duration > 0 ? Math.max(0, timer.remaining) / timer.duration : 0;
        const elapsedAngle = (1 - progress) * 360;

        const stateClass =
          timer.state === "Expired"
            ? "buff-expired"
            : timer.state === "Warning"
              ? "buff-warning"
              : "buff-active";

        const abbr = timer.name.slice(0, 2);

        return (
          <div key={timer.id} className={`buff-icon ${stateClass}`}>
            <span className="buff-emoji-text">{abbr}</span>
            <span className="buff-secs">
              {timer.state === "Expired" ? "!" : displayTime}
            </span>
            {timer.state !== "Expired" && (
              <div
                className="buff-sweep"
                style={{
                  background: `conic-gradient(rgba(0,0,0,0.6) ${elapsedAngle}deg, transparent ${elapsedAngle}deg)`,
                }}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

// --- App ---
function App() {
  const [timers, setTimers] = useState<Timer[]>([]);
  const [activeBoss, setActiveBoss] = useState<string | null>(null);
  const [bossConfig, setBossConfig] = useState<BossConfig | null>(null);
  const [hiddenTimers, setHiddenTimers] = useState<string[]>([]);
  const [mutedTimers, setMutedTimers] = useState<string[]>([]);
  const [miniMode, setMiniMode] = useState(false);
  const [bosses, setBosses] = useState<BossListItem[]>([]);
  const [showPicker, setShowPicker] = useState(true);
  const [showSettings, setShowSettings] = useState(false);
  const [showBuffForm, setShowBuffForm] = useState(false);
  const [editingBuff, setEditingBuff] = useState<BuffItem | null>(null);
  const [hotkeyOverrides, setHotkeyOverrides] = useState<Record<string, string>>({});
  const [homeTab, setHomeTab] = useState<"boss" | "buff">("boss");

  // Load boss list on mount
  useEffect(() => {
    invoke<BossListItem[]>("list_bosses").then(setBosses);
  }, []);

  // Listen for timer updates
  useEffect(() => {
    const unlisten = listen<TimerUpdate>("timer-update", (event) => {
      const sorted = [...event.payload.timers].sort((a, b) => {
        if (a.state === "Expired" && b.state !== "Expired") return 1;
        if (a.state !== "Expired" && b.state === "Expired") return -1;
        return a.remaining - b.remaining;
      });
      setTimers(sorted);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Listen for back-to-main event from Rust (hotkey)
  useEffect(() => {
    const unlisten = listen("back-to-main", () => {
      setActiveBoss(null);
      setBossConfig(null);
      setShowPicker(true);
      setShowSettings(false);
      setTimers([]);
      resizeRightAnchored(SIZE_PICKER);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Disable global shortcuts while text inputs are focused to prevent hotkey eating
  useEffect(() => {
    const handleFocusIn = (e: FocusEvent) => {
      const target = e.target as HTMLElement;
      if (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target.classList.contains("hotkey-capture")
      ) {
        invoke("disable_shortcuts");
      }
    };
    const handleFocusOut = (e: FocusEvent) => {
      const target = e.target as HTMLElement;
      if (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target.classList.contains("hotkey-capture")
      ) {
        invoke("enable_shortcuts");
      }
    };
    document.addEventListener("focusin", handleFocusIn);
    document.addEventListener("focusout", handleFocusOut);
    return () => {
      document.removeEventListener("focusin", handleFocusIn);
      document.removeEventListener("focusout", handleFocusOut);
    };
  }, []);

  const selectBoss = async (bossId: string) => {
    const resp = await invoke<SelectBossResponse>("select_boss", { bossId });
    setActiveBoss(bossId);
    setBossConfig(resp.config);
    setHiddenTimers(resp.hidden_timers);
    setMutedTimers(resp.muted_timers);
    setMiniMode(resp.mini_mode);
    setShowPicker(false);
    setTimers([]);

    // Load hotkey overrides
    const settings = await invoke<AppSettings>("get_settings");
    setHotkeyOverrides(settings.hotkeys[bossId] || {});

    resizeRightAnchored(resp.mini_mode ? SIZE_DETAIL_MINI : SIZE_DETAIL);
  };

  const goBackToMain = useCallback(async () => {
    await invoke("stop_all_timers");
    setActiveBoss(null);
    setBossConfig(null);
    setShowPicker(true);
    setShowSettings(false);
    setTimers([]);
    setHomeTab("boss");
    resizeRightAnchored(SIZE_PICKER);
  }, []);

  const handleHideTimer = async (timerId: string) => {
    if (!activeBoss) return;
    await invoke("hide_timer", { bossId: activeBoss, timerId });
    setHiddenTimers((prev) => [...prev, timerId]);
  };

  const handleResetHidden = async () => {
    if (!activeBoss) return;
    await invoke("reset_hidden_timers", { bossId: activeBoss });
    setHiddenTimers([]);
  };

  const handleToggleMute = async (timerId: string) => {
    if (!activeBoss) return;
    const isMuted = await invoke<boolean>("toggle_mute_timer", { bossId: activeBoss, timerId });
    setMutedTimers((prev) =>
      isMuted ? [...prev, timerId] : prev.filter((id) => id !== timerId)
    );
  };

  const handleToggleMini = async () => {
    const newMode = !miniMode;
    await invoke("set_mini_mode", { enabled: newMode });
    setMiniMode(newMode);
    resizeRightAnchored(newMode ? SIZE_DETAIL_MINI : SIZE_DETAIL);
  };

  const closeApp = () => {
    getCurrentWindow().close();
  };

  // Show settings page
  if (showSettings) {
    return (
      <SettingsPage
        bosses={bosses}
        onBack={() => {
          setShowSettings(false);
          resizeRightAnchored(SIZE_PICKER);
        }}
      />
    );
  }

  // Show buff form page
  if (showBuffForm) {
    return (
      <BuffFormPage
        editBuff={editingBuff}
        onSave={() => {
          setShowBuffForm(false);
          setEditingBuff(null);
          setHomeTab("buff");
          resizeRightAnchored(SIZE_PICKER);
        }}
        onCancel={() => {
          setShowBuffForm(false);
          setEditingBuff(null);
          setHomeTab("buff");
          resizeRightAnchored(SIZE_PICKER);
        }}
      />
    );
  }

  // Show boss detail page when a boss is selected
  if (activeBoss && bossConfig) {
    return (
      <BossDetailPage
        config={bossConfig}
        timers={timers.filter((t) => t.timer_type !== "buff")}
        hiddenTimers={hiddenTimers}
        mutedTimers={mutedTimers}
        miniMode={miniMode}
        hotkeys={hotkeyOverrides}
        onBack={goBackToMain}
        onHideTimer={handleHideTimer}
        onToggleMute={handleToggleMute}
        onResetHidden={handleResetHidden}
        onToggleMini={handleToggleMini}
      />
    );
  }

  // Show homepage with Boss/Buff tabs when no boss is selected
  if (showPicker && !activeBoss) {
    return (
      <div className="boss-picker">
        <div className="window-header" onMouseDown={startDrag}>
          <div className="picker-title">Artale Timer</div>
          <button className="close-btn" onClick={closeApp} onMouseDown={(e) => e.stopPropagation()}>✕</button>
        </div>
        <div className="home-tabs">
          <button
            className={`home-tab ${homeTab === "boss" ? "home-tab-active" : ""}`}
            onClick={() => setHomeTab("boss")}
          >
            Boss
          </button>
          <button
            className={`home-tab ${homeTab === "buff" ? "home-tab-active" : ""}`}
            onClick={() => setHomeTab("buff")}
          >
            Buff
          </button>
        </div>
        {homeTab === "boss" ? (
          <>
            <div className="picker-subtitle">選擇 Boss / Select Boss</div>
            <div className="boss-list">
              {bosses.map((boss) => (
                <button
                  key={boss.id}
                  className="boss-button"
                  onClick={() => selectBoss(boss.id)}
                >
                  <span className="boss-name">{boss.name}</span>
                  <span className="boss-desc">{boss.description}</span>
                  <span className="boss-timers">{boss.timer_count} timers</span>
                </button>
              ))}
            </div>
          </>
        ) : (
          <BuffTabContent
            onAdd={() => {
              setEditingBuff(null);
              setShowBuffForm(true);
              resizeRightAnchored(SIZE_BUFF_FORM);
            }}
            onEdit={(buff) => {
              setEditingBuff(buff);
              setShowBuffForm(true);
              resizeRightAnchored(SIZE_BUFF_FORM);
            }}
          />
        )}
        <button
          className="settings-link"
          onClick={() => {
            setShowSettings(true);
            resizeRightAnchored(SIZE_SETTINGS);
          }}
        >
          快捷鍵設定 / Hotkey Settings
        </button>
      </div>
    );
  }

  return null;
}

// --- Root: route to the correct component based on window label ---
function Root() {
  const [windowLabel] = useState(() => getCurrentWindow().label);

  if (windowLabel === "buff-hud") {
    return <BuffHudApp />;
  }
  return <App />;
}

export default Root;
