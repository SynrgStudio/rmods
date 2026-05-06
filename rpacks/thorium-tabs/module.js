export default function createModule() {
  return {
    onQueryChange(query, ctx) {
      const normalized = query.trim().toLowerCase();
      if (normalized !== "thorium tabs" && normalized !== "tabs thorium") return;

      ctx.setInputAccessory({
        text: "thorium-tabs resident helper: Alt+mouse gestures control Thorium tabs",
        kind: "info"
      });
      ctx.replaceItems([
        {
          id: "thorium-tabs::status",
          title: "Thorium Tabs",
          subtitle: "Resident helper managed by rmenu-daemon",
          source: "thorium-tabs",
          badge: "resident"
        }
      ]);
    }
  };
}
