// 日本株版の通貨フォーマッタ（円 / JPY）
const jpyFormatter = new Intl.NumberFormat("ja-JP", {
  style: "currency",
  currency: "JPY",
  maximumFractionDigits: 2,
});

export function formatJPY(value: number): string {
  return jpyFormatter.format(value);
}
