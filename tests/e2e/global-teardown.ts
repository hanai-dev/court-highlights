import { saveServiceLogs } from './specs/helpers';

/** The last containers' logs, for the log privacy scan after a full run. */
export default function globalTeardown(): void {
  saveServiceLogs();
}
