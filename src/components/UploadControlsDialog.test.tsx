import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { UploadControlsDialog, UploadControlsReview, UploadWindowFields } from "./UploadControlsDialog";
vi.mock("../lib/local",()=>({loadConnectionSettings:vi.fn()}));
vi.mock("../lib/upload-controls",async(original)=>({...await original<typeof import("../lib/upload-controls")>(),loadUploadControls:vi.fn(),saveUploadControls:vi.fn()}));
describe("stepped transfer controls",()=>{
 it("opens Budget then Hours and priority then Review with loading and close controls",()=>{
  const markup=renderToStaticMarkup(<UploadControlsDialog expectedChannelId="channel-a" onClose={()=>undefined}/>);
  for(const text of ['aria-modal="true"','aria-label="Upload control steps"',"Budget","Hours &amp; priority","Review","Set a shared upload budget","Loading this channel’s transfer choices","Close upload controls"])expect(markup).toContain(text);
  expect(markup).not.toContain("Save upload controls");
 });
 it("shows every selected local weekday and overnight meaning with accessible controls",()=>{
  const markup=renderToStaticMarkup(<UploadWindowFields id="window" label="Upload window 1" value={{days:[1,5],start:"22:00",end:"02:00"}} onChange={()=>undefined} onRemove={()=>undefined}/>);
  expect(markup).toContain("continues into the next day");expect(markup).toContain('id="window-start"');expect(markup).toContain('value="22:00"');expect(markup).toContain('aria-label="Remove upload window 1"');
 });
 it("reviews device budget, timed windows and selected scoped priorities before mutation",()=>{
  const markup=renderToStaticMarkup(<UploadControlsReview settings={{bytesPerSecond:1_250_000,windows:[{days:[1],start:"22:00",end:"02:00"}],timedLimits:[{days:[1],start:"09:00",end:"17:00",bytesPerSecond:625_000}]}} priorities={[{kind:"folder",id:"folder-a",priority:5}]} targets={[{kind:"folder",id:"folder-a",title:"Camera A <originals>",priority:0,queuedItems:2}]} timeZone="Device local time · UTC+08:00"/>);
  for(const text of ["10 Mbps","5 Mbps","ends next day","Camera A &lt;originals&gt;","Priority 5","All workers share it","saved checkpoint","future selection","older waiting work keeps a turn"])expect(markup).toContain(text);
 });
 it("shows any-time and unlimited defaults without suggesting a closed app runs",()=>{
  const markup=renderToStaticMarkup(<UploadControlsReview settings={{windows:[],timedLimits:[]}} priorities={[]} targets={[]} timeZone="Device local time"/>);
  expect(markup).toContain("Unlimited");expect(markup).toContain("Any time while this app is running");expect(markup).toContain("No timed budgets");expect(markup).toContain("Normal priority");
 });
});
