import { useState, useCallback, useEffect, useRef, type ReactNode } from "react";

interface ResizablePanelProps {
  children: ReactNode;
  minHeight?: number;
  maxHeight?: number;
  defaultHeight?: number;
  storageKey?: string;
}

export function ResizablePanel({
  children,
  minHeight = 100,
  maxHeight = 600,
  defaultHeight = 200,
  storageKey = "detailPanelHeight",
}: ResizablePanelProps) {
  const [height, setHeight] = useState(() => {
    if (storageKey) {
      const stored = localStorage.getItem(storageKey);
      if (stored) {
        const parsed = parseInt(stored, 10);
        if (!isNaN(parsed)) {
          return Math.min(Math.max(parsed, minHeight), maxHeight);
        }
      }
    }
    return defaultHeight;
  });

  const [isDragging, setIsDragging] = useState(false);
  const panelRef = useRef<HTMLDivElement>(null);
  const startYRef = useRef(0);
  const startHeightRef = useRef(0);

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      setIsDragging(true);
      startYRef.current = e.clientY;
      startHeightRef.current = height;
    },
    [height]
  );

  const handleMouseMove = useCallback(
    (e: MouseEvent) => {
      if (!isDragging) return;

      const deltaY = startYRef.current - e.clientY;
      const newHeight = Math.min(Math.max(startHeightRef.current + deltaY, minHeight), maxHeight);
      setHeight(newHeight);
    },
    [isDragging, minHeight, maxHeight]
  );

  const handleMouseUp = useCallback(() => {
    if (isDragging) {
      setIsDragging(false);
      if (storageKey) {
        localStorage.setItem(storageKey, height.toString());
      }
    }
  }, [isDragging, height, storageKey]);

  useEffect(() => {
    if (isDragging) {
      document.addEventListener("mousemove", handleMouseMove);
      document.addEventListener("mouseup", handleMouseUp);
      document.body.style.cursor = "ns-resize";
      document.body.style.userSelect = "none";
    }

    return () => {
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
  }, [isDragging, handleMouseMove, handleMouseUp]);

  return (
    <div
      ref={panelRef}
      className="resizable-panel"
      style={{ height: `${height}px` }}
    >
      <div
        className={`resizable-panel__handle ${isDragging ? "resizable-panel__handle--active" : ""}`}
        onMouseDown={handleMouseDown}
      >
        <div className="resizable-panel__handle-bar" />
      </div>
      <div className="resizable-panel__content">{children}</div>
    </div>
  );
}

export default ResizablePanel;
