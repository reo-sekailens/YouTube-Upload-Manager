import { beforeEach, describe, expect, it, vi } from "vitest";
import { loadUploadControls, saveUploadControls, uploadBudgetFromMbps, uploadBudgetInput, uploadBudgetMbps, uploadControlsMatchesChannel, uploadControlsValidation, uploadWindowLabel, type UploadControlsOverview, type UploadTransferSettings } from "./upload-controls";
const mocks=vi.hoisted(()=>({invoke:vi.fn(),isTauri:vi.fn(()=>true)}));
vi.mock("@tauri-apps/api/core",()=>({invoke:mocks.invoke,isTauri:mocks.isTauri}));
const settings:UploadTransferSettings={bytesPerSecond:1_250_000,windows:[{days:[1,2,3,4,5],start:"09:00",end:"17:00"}],timedLimits:[]};
const overview:UploadControlsOverview={channelId:"channel-a",settings,priorities:[],targets:[{kind:"batch",id:"batch-a",title:"Event",priority:0,queuedItems:2}],timeZone:"Device local time · UTC+08:00",currentOffsetMinutes:480,timezoneChanged:false,status:{allowed:true}};
describe("device upload controls",()=>{
 beforeEach(()=>{mocks.invoke.mockReset();mocks.isTauri.mockReturnValue(true);});
 it("preserves expected channel and explicit final approval for complete settings",async()=>{
  await loadUploadControls("channel-a");await saveUploadControls("channel-a",settings,[{kind:"batch",id:"batch-a",priority:5}],true);
  expect(mocks.invoke.mock.calls).toEqual([["load_upload_controls",{expectedChannelId:"channel-a"}],["save_upload_controls",{expectedChannelId:"channel-a",settings,priorities:[{kind:"batch",id:"batch-a",priority:5}],approved:true}]]);
 });
 it("never fabricates saved transfer controls in browser mode",async()=>{mocks.isTauri.mockReturnValue(false);await expect(loadUploadControls("channel-a")).rejects.toThrow("desktop app");expect(mocks.invoke).not.toHaveBeenCalled();});
 it("converts internet Mbps and bytes without treating them as megabytes",()=>{expect(uploadBudgetFromMbps("10")).toBe(1_250_000);expect(uploadBudgetMbps(1_250_000)).toBe("10 Mbps");expect(uploadBudgetInput(128_000)).toBe("1.024");expect(uploadBudgetMbps()).toBe("Unlimited");});
 it("validates rates, local window bounds and explicit weekdays before review",()=>{
  expect(uploadControlsValidation(settings,[])).toBeUndefined();
  for(const next of [{...settings,bytesPerSecond:0},{...settings,bytesPerSecond:NaN},{...settings,windows:[{days:[],start:"09:00",end:"17:00"}]},{...settings,windows:[{days:[1,1],start:"09:00",end:"17:00"}]},{...settings,windows:[{days:[1],start:"24:00",end:"17:00"}]},{...settings,windows:[{days:[1],start:"09:00",end:"09:00"}]}]) expect(uploadControlsValidation(next,[])).toBeTruthy();
 });
 it("permits overnight windows but describes the selected start day",()=>{const window={days:[1,5],start:"22:00",end:"02:00"};expect(uploadControlsValidation({...settings,windows:[window]},[])).toBeUndefined();expect(uploadWindowLabel(window)).toBe("Mon, Fri · 22:00–02:00 (ends next day)");});
 it("rejects duplicate target priorities and malformed channel projections",()=>{
  const choice={kind:"batch" as const,id:"batch-a",priority:5};expect(uploadControlsValidation(settings,[choice,choice])).toBeTruthy();expect(uploadControlsValidation(settings,[{...choice,priority:6}])).toBeTruthy();
  expect(uploadControlsMatchesChannel(overview,"channel-a")).toBe(true);expect(uploadControlsMatchesChannel(overview,"channel-b")).toBe(false);expect(uploadControlsMatchesChannel({...overview,targets:[...overview.targets,...overview.targets]},"channel-a")).toBe(false);
 });
});
