// The system Diskman runs on, as the backend reports it (commands/admin.rs): administrator rights
// and whether Diskman can restart itself with them. The OS itself is known before (src/platform).
import { backend } from '../api/backend.ts';
import { OS } from '../platform/index.ts';

const state = $state({ elevated: false, canElevate: false });

export const platform = {
  os: OS,
  /** Runs with administrator rights (Windows: elevated; Linux: root). */
  get elevated() {
    return state.elevated;
  },
  /** Can restart with administrator rights (Windows only, while not elevated). */
  get canElevate() {
    return state.canElevate;
  },
};

export async function loadPlatform() {
  try {
    const info = await backend.platform();
    state.elevated = info.elevated;
    state.canElevate = info.can_elevate;
  } catch {
    // Without an answer nothing is offered that needs it
  }
}
