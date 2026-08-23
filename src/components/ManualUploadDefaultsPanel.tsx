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
  return <section className="mb-4 flex items-center justify-between gap-4 rounded-lg border border-[#dfe6ef] bg-[#f7f9fc] px-3.5 py-3 max-sm:flex-col max-sm:items-stretch" aria-labelledby="manual-upload-defaults-heading"><div><p className="mb-1 text-[0.67rem] font-bold tracking-[0.1em] text-[#68748a] uppercase">DEVICE-WIDE UPLOAD DEFAULT</p><h3 className="my-0.5 text-[0.86rem] text-[#30415c]" id="manual-upload-defaults-heading">Made for Kids</h3><p className="mt-1 mb-0 text-[0.73rem] leading-snug text-[#718095]">Prefills every manual drop review. Each batch still shows this choice before import.</p></div><div className="grid min-w-[13rem] gap-1.5"><label className="grid gap-1 text-[0.68rem] font-bold text-[#56667e] uppercase"><span>Default</span><select className="rounded-md border border-[#cbd5e3] bg-white px-2 py-1.5 text-[0.76rem] text-[#324966] focus:border-[#2463df] focus:outline-3 focus:outline-[#2d68e8]/15" disabled={!isTauri || saving} onChange={(event) => void save(event.target.value === "yes")} value={madeForKids ? "yes" : "no"}><option value="yes">Yes, made for kids</option><option value="no">No, not made for kids</option></select></label>{notice && <p className="m-0 text-[0.69rem] text-[#4f6690]" role="status">{notice}</p>}</div></section>;
}
