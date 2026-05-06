export default function createModule() {
  return {
    onQueryChange(query, ctx) {
      const normalized = query.trim().toLowerCase();
      if (normalized !== "taskbar volume" && normalized !== "volume taskbar") return;

      ctx.setInputAccessory({
        text: "taskbar-volume resident helper: wheel over taskbar controls volume",
        kind: "info"
      });
      ctx.replaceItems([
        {
          id: "taskbar-volume::status",
          title: "Taskbar Volume",
          subtitle: "Resident helper managed by rmenu-daemon",
          source: "taskbar-volume",
          badge: "resident"
        }
      ]);
    }
  };
}
