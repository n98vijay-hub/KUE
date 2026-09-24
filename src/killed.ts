/** Whether the window must show the stopped surface.
 *
 * Fail closed, and deliberately not derived from one channel. The projection
 * arrives on a tick whose failure is swallowed; the context object arrives on
 * an event. If the projection stopped arriving while context events kept
 * coming, a window that trusted only the projection would go on rendering the
 * last good one — a trust strip reading "Camera on" included — with KUE already
 * stopped. The brief's requirement is that the kill state is unmistakable, and
 * one channel is not a basis for that.
 *
 * Either source saying stopped is enough. Neither can say "not stopped" over
 * the other.
 */
export function isKilled(
  surface: { system: string } | null,
  ctx: { runtime: { state: string } } | null,
): boolean {
  const bySurface = surface?.system === "STOPPED" || surface?.system === "RECOVERING";
  const byContext = ctx?.runtime.state === "KUE_KILLED" || ctx?.runtime.state === "KUE_RECOVERING";
  return bySurface || byContext;
}
