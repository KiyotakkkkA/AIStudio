import { SystemService } from "../../apps/studio/src/host/services/SystemService.ts";

export function createSystemService(): SystemService {
  return new SystemService({
    async chunk() {
      return [];
    },
    async hash() {
      return "";
    },
  });
}
