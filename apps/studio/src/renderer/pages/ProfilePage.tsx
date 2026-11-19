import { useOutletContext } from "react-router-dom";
import { RAIL_IDENTITY } from "../app/navigation";
import ProfileActivity from "../features/profile/ProfileActivity";

export default function ProfilePage() {
  const avatarUrl = useOutletContext<string | null>();
  return <ProfileActivity identity={RAIL_IDENTITY} avatarUrl={avatarUrl} />;
}
