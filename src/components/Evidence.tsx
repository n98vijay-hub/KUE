import type { ConfidenceBreakdown, EvidenceItem } from "../types";
import { SOURCE_LABEL } from "../types";

/**
 * Shows the full working behind a confidence number.
 *
 * This is the honesty guarantee made visible: the rows below are exactly the
 * rows the core counted, so anyone can redo the arithmetic by hand and land on
 * the same value. Nothing is rounded away or hidden.
 */
export function Evidence({
  items,
  confidence,
}: {
  items: EvidenceItem[];
  confidence: ConfidenceBreakdown;
}) {
  if (items.length === 0) {
    return <p className="empty">No evidence is available right now.</p>;
  }
  return (
    <div>
      {items.map((e) => {
        const contribution = e.weight * e.strength * e.reliability;
        const negative = e.polarity === "contradicts";
        return (
          <div className="ev-row" key={e.id}>
            <div className={`ev-mark ${e.polarity}`}>{negative ? "−" : "+"}</div>
            <div>
              <div className="ev-text">{e.statement}</div>
              <div className="ev-meta">
                {SOURCE_LABEL[e.source]} · weight {e.weight.toFixed(2)} × strength{" "}
                {e.strength.toFixed(3)} × reliability {e.reliability.toFixed(2)}
              </div>
            </div>
            <div className={`ev-math ${negative ? "neg" : ""}`}>
              {negative ? "−" : "+"}
              {contribution.toFixed(3)}
            </div>
          </div>
        );
      })}

      <div className="ev-total">
        <div />
        <div>
          supporting {confidence.supporting.toFixed(3)} − contradicting{" "}
          {confidence.contradicting.toFixed(3)} ÷ capacity{" "}
          {confidence.denominator.toFixed(3)}
        </div>
        <div className="ev-math">{confidence.value.toFixed(3)}</div>
      </div>

      <div className="conf-formula">{confidence.formula}</div>
    </div>
  );
}

export function ConfidenceMeter({
  label,
  confidence,
}: {
  label: string;
  confidence: ConfidenceBreakdown;
}) {
  return (
    <div className="conf">
      <div className="conf-head">
        <span className="label">{label}</span>
        <span className="conf-value">{(confidence.value * 100).toFixed(0)}%</span>
      </div>
      <div className="meter">
        <div className="meter-fill" style={{ width: `${confidence.value * 100}%` }} />
      </div>
    </div>
  );
}
