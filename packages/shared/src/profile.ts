import { z } from "zod";

export const ProfileAvatarDto = z.object({ svg: z.string().min(1).max(4096) });
export type ProfileAvatarDto = z.infer<typeof ProfileAvatarDto>;
