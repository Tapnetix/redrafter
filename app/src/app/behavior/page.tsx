// Standalone route so the Behavior screen (app/src/screens/Behavior.tsx)
// can be visited directly in isolation before A14 wires the real app
// router (App.tsx) and shared NavRail chrome around every settings screen.
import Behavior from '@/screens/Behavior';

export default function BehaviorPage() {
  return <Behavior />;
}
