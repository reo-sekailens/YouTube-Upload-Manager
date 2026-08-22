import { useEffect, useState } from "react";
import { isTauri, loadManualUploadDefaults, saveManualUploadDefaults } from "../lib/local";

export function ManualUploadDefaultsPanel() {
  const [madeForKids, setMadeForKids] = useState(false);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState("");

  useEffect(() => { void loadManualUploadDefaults().then((defaults) => setMadeForKids(defaults.madeForKids)).catch(() => setNotice("The saved intake default could not be loaded.")); }, []);
  const save = async (nextValue: boolean) => {
    if (!isTauri) return;
    setSaving(true);
    try {
      const saved = await saveManualUploadDefaults(nextValue);
      setMadeForKids(saved.madeForKids);
      setNotice(`Default saved: ${saved.madeForKids ? "Made for Kids" : "Not made for kids"}.`);
    } catch (error) { setNotice(error instanceof Error ? error.message : "The default could not be saved."); }
    finally { setSaving(false); }
  };
  return <section className="manual-upload-defaults" aria-labelledby="manual-upload-defaults-heading"><div><p className="eyebrow">DEVICE-WIDE UPLOAD DEFAULT</p><h3 id="manual-upload-defaults-heading">Made for Kids</h3><p>Prefills every manual drop review. Each batch still shows this choice before import.</p></div><div className="manual-upload-defaults__actions"><label><span>Default</span><select disabled={!isTauri || saving} onChange={(event) => void save(event.target.value === "yes")} value={madeForKids ? "yes" : "no"}><option value="yes">Yes, made for kids</option><option value="no">No, not made for kids</option></select></label>{notice && <p role="status">{notice}</p>}</div></section>;
}
