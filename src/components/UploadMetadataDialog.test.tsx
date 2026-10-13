import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { UploadMetadataDialog, UploadMetadataFields, UploadMetadataPreview } from "./UploadMetadataDialog";
vi.mock("../lib/local", () => ({ loadConnectionSettings: vi.fn() }));
vi.mock("../lib/upload-metadata", async (original) => ({ ...await original<typeof import("../lib/upload-metadata")>(), loadUploadPresets: vi.fn(), saveUploadPreset: vi.fn(), deleteUploadPreset: vi.fn(), resolveUploadMetadata: vi.fn(), mapUploadManifest: vi.fn() }));

describe("stepped upload metadata review", () => {
  it("opens with sources before editing or applying and keeps accessible steps", () => {
    const markup = renderToStaticMarkup(<UploadMetadataDialog expectedChannelId="channel-a" paths={["D:/Session/a.mp4"]} onApply={() => undefined} onClose={() => undefined} />);
    for (const text of ['aria-modal="true"', 'aria-label="Metadata steps"', "Preset or manifest", "Edit", "Review", "Choose metadata sources", "Choose matching root", "Choose manifest", "Next: Edit"]) expect(markup).toContain(text);
    expect(markup).not.toContain(">Apply metadata<");
    expect(markup).toContain("changing a preset leaves existing queues intact");
  });
  it("keeps absent optional declarations separate from explicit false and empty values", () => {
    const absent = renderToStaticMarkup(<UploadMetadataFields id="test" metadata={{}} onChange={() => undefined} />);
    expect(absent).toContain("No description declaration"); expect(absent).not.toContain('id="test-description"');
    const explicit = renderToStaticMarkup(<UploadMetadataFields id="test" metadata={{ description: "", tags: [], notifySubscribers: false, containsSyntheticMedia: false, license: "youtube" }} onChange={() => undefined} />);
    expect(explicit).toContain('id="test-description"'); expect(explicit).toContain('value="false" selected=""'); expect(explicit).toContain('value="youtube" selected=""');
    expect(explicit).toContain("An empty description is an explicit choice");
  });
  it("shows all resolved metadata and declarations per video with escaped content", () => {
    const markup = renderToStaticMarkup(<UploadMetadataPreview fileName="演奏.mp4" sequence={2} override metadata={{ title: "演奏 & session", description: "Line one\nLine two", tags: ["music", "two words"], categoryId: "10", language: "ja", defaultAudioLanguage: "ja", license: "creativeCommon", notifySubscribers: false, containsSyntheticMedia: true }} />);
    for (const text of ["2. 演奏 &amp; session", "Item override", "Line one", "Line two", "music · two words", "Creative Commons attribution", "Do not notify", "Yes", "Metadata language", "Audio language"]) expect(markup).toContain(text);
  });
  it("shows filename fallback, default notifications and no invented declarations", () => {
    const markup = renderToStaticMarkup(<UploadMetadataPreview fileName="clip.mp4" sequence={1} metadata={{}} />);
    expect(markup).toContain("1. clip"); expect(markup).toContain("YouTube default · notify"); expect(markup).toContain("No declaration"); expect(markup).not.toContain("Standard YouTube");
  });
  it("displays existing calendar choices without a second independent schedule editor", () => {
    const markup = renderToStaticMarkup(<UploadMetadataFields id="test" metadata={{ publishAt: "2099-10-13T04:00:00Z" }} onChange={() => undefined} />);
    expect(markup).toContain("2099-10-13T04:00:00Z"); expect(markup).toContain("private upload visibility is required"); expect(markup).not.toContain('type="datetime-local"');
  });
});
