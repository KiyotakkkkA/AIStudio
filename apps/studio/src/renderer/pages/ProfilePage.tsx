import { RAIL_IDENTITY } from "../app/navigation";
import ProfileActivity from "../features/profile/ProfileActivity";

export default function ProfilePage() {
  return <ProfileActivity identity={RAIL_IDENTITY} />;
}
