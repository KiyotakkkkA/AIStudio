import { expect, test } from "vitest";
import { toolImageReferences } from "../../src/renderer/features/chat/imageData";

const owner = "0199bb11-4444-7111-8111-000000000001";
const image = {
  id: "0199bb11-4444-7111-8111-000000000002",
  name: "screen-test-123.jpg",
  mimeType: "image/jpeg",
  sizeBytes: 4,
};

test("tool previews require an owner and validated image references", () => {
  expect(toolImageReferences({ imageOwner: owner, images: [image] })).toEqual({
    owner,
    images: [image],
  });
  expect(toolImageReferences({ imageOwner: "../outside", images: [image] })).toBeNull();
  expect(
    toolImageReferences({ imageOwner: owner, images: [{ ...image, mimeType: "text/html" }] }),
  ).toBeNull();
  expect(toolImageReferences({ imageOwner: owner, images: [] })).toBeNull();
  expect(toolImageReferences(undefined)).toBeNull();
});
