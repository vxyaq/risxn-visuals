import { useEffect, useState } from 'react';
import { ChevronDown, Minus, X } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

type VisualSettings = { brightness: number; contrast: number; saturation: number; hue: number };
type Profile = VisualSettings & { id: string; name: string; updatedAt: number };

const defaults: VisualSettings = { brightness: 0, contrast: 100, saturation: 100, hue: 0 };
const initialProfiles: Profile[] = [];

const read = <T,>(key: string, fallback: T): T => {
  try {
    const saved = localStorage.getItem(key);
    return saved ? JSON.parse(saved) as T : fallback;
  } catch {
    return fallback;
  }
};

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

function App() {
  const [settings, setSettings] = useState<VisualSettings>(() => read('ether-settings', defaults));
  const [profiles, setProfiles] = useState<Profile[]>(() => read('ether-profiles', initialProfiles));
  const [activeId, setActiveId] = useState(() => read('ether-active', ''));
  const [profileName, setProfileName] = useState('');
  const window = getCurrentWindow();

  useEffect(() => {
    localStorage.setItem('ether-settings', JSON.stringify(settings));

    const brightnessCSS = 1 + settings.brightness / 100;
    const contrastCSS = settings.contrast / 100;
    const saturationCSS = settings.saturation / 100;
    
    document.documentElement.style.filter = `
      brightness(${brightnessCSS}) 
      contrast(${contrastCSS}) 
      saturate(${saturationCSS}) 
      hue-rotate(${settings.hue}deg)
    `;

    invoke('apply_visuals', { settings }).catch((err) => {
      console.error('Error applying visuals:', err);
    });
  }, [settings]);

  useEffect(() => localStorage.setItem('ether-profiles', JSON.stringify(profiles)), [profiles]);
  useEffect(() => localStorage.setItem('ether-active', JSON.stringify(activeId)), [activeId]);

  const activeProfile = profiles.find((profile) => profile.id === activeId);

  const update = (key: keyof VisualSettings, value: number) => setSettings((current) => ({ ...current, [key]: value }));

  const loadProfile = (id: string) => {
    const next = profiles.find((profile) => profile.id === id);
    if (!next) return;
    setActiveId(id);
    setSettings({ brightness: next.brightness, contrast: next.contrast, saturation: next.saturation, hue: next.hue });
  };

  const saveProfile = () => {
    if (!activeProfile) return;
    setProfiles((current) => current.map((profile) => profile.id === activeId ? { ...profile, ...settings, updatedAt: Date.now() } : profile));
  };

  const createProfile = () => {
    const name = profileName.trim();
    if (!name) return;
    const next: Profile = { id: `${Date.now()}`, name, ...settings, updatedAt: Date.now() };
    setProfiles((current) => [...current, next]);
    setActiveId(next.id);
    setProfileName('');
  };

  const formatMultiplier = (val: number) => (val / 100).toFixed(2);
  const formatBrightness = (val: number) => (val / 100).toFixed(2);

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
             <path d="M4 6H20M4 12H20M4 18H20" stroke="white" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" style={{ transform: 'skewX(-15deg)' }}/>
          </svg>
          <span>Risxn Visuals</span>
        </div>
        <div className="topbar-actions">
          <button className="window-button minimize" title="Minimize" onClick={() => void window.minimize()}><Minus size={16} strokeWidth={2.5} /></button>
          <button className="window-button close" title="Close" onClick={() => void window.close()}><X size={16} strokeWidth={2.5} /></button>
        </div>
      </header>

      <div className="workspace-grid">
        <section className="controls-panel">
          <Slider label="Brightness" value={settings.brightness} min={-50} max={50} display={formatBrightness(settings.brightness)} onChange={(value) => update('brightness', value)} />
          <Slider label="Contrast" value={settings.contrast} min={50} max={150} display={formatMultiplier(settings.contrast)} onChange={(value) => update('contrast', value)} />
          <Slider label="Saturation" value={settings.saturation} min={0} max={200} display={formatMultiplier(settings.saturation)} onChange={(value) => update('saturation', value)} />
          <Slider label="Hue" value={settings.hue} min={-180} max={180} display={`${settings.hue}°`} onChange={(value) => update('hue', value)} />
        </section>

        <div className="vertical-divider" />

        <aside className="profiles-panel">
          <h2>Profiles</h2>
          
          <div className="form-group">
            <label className="field-label" htmlFor="profile-select">Load profile</label>
            <div className="select-wrap">
              <select id="profile-select" value={activeId} onChange={(event) => loadProfile(event.target.value)}>
                <option value="" disabled hidden>Select profile...</option>
                {profiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.name}</option>)}
              </select>
              <ChevronDown size={14} className="select-icon" />
            </div>
            <button className="outline-button" onClick={saveProfile} disabled={!activeProfile}>Save current to profile</button>
          </div>

          <div className="form-group create-group">
            <label className="field-label" htmlFor="profile-name">Create new profile</label>
            <input id="profile-name" type="text" value={profileName} onChange={(event) => setProfileName(event.target.value)} onKeyDown={(event) => event.key === 'Enter' && createProfile()} placeholder="Profile name" maxLength={30} />
            <button className="solid-button" onClick={createProfile} disabled={!profileName.trim()}>Create profile</button>
          </div>

        </aside>
      </div>
    </main>
  );
}

function Slider({ label, value, min, max, display, onChange }: { label: string; value: number; min: number; max: number; display: string; onChange: (value: number) => void }) {
  const percentage = ((value - min) / (max - min)) * 100;
  return (
    <div className="slider-row">
      <div className="slider-meta">
        <label>{label}</label>
        <output>{display}</output>
      </div>
      <input aria-label={label} type="range" min={min} max={max} value={value} onChange={(event) => onChange(clamp(Number(event.target.value), min, max))} style={{ '--progress': `${percentage}%` } as React.CSSProperties} />
    </div>
  );
}

export default App;
