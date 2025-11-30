import * as React from 'react';
import { gsap } from 'gsap';
import { getStateColor, formatState } from '../utils';
import type { TorrentState } from '../types';

// States that represent active transfers (should pulse based on speed)
const ACTIVE_TRANSFER_STATES: TorrentState[] = [
  'downloading',
  'forcedDL',
  'uploading',
  'forcedUP',
  'metaDL',
  'forcedMetaDL',
];

// States that represent background activity (should pulse at steady rate)
const BACKGROUND_ACTIVITY_STATES: TorrentState[] = [
  'checkingDL',
  'checkingUP',
  'checkingResumeData',
  'allocating',
  'moving',
];

// Speed thresholds (in KB/s) for determining pulse rate
const SPEED_THRESHOLDS = {
  VERY_SLOW: 10,      // < 10 KB/s
  SLOW: 100,          // 10-100 KB/s
  MEDIUM: 1024,       // 100 KB/s - 1 MB/s
  FAST: 10240,        // 1-10 MB/s
} as const;

// Pulse durations (in seconds) corresponding to speed ranges
const PULSE_DURATIONS = {
  VERY_SLOW: 2.5,     // < 10 KB/s
  SLOW: 2.0,          // 10-100 KB/s
  MEDIUM: 1.5,        // 100 KB/s - 1 MB/s
  FAST: 1.0,          // 1-10 MB/s
  VERY_FAST: 0.5,     // > 10 MB/s
  BACKGROUND: 1.5,    // Background tasks (checking, allocating)
  NO_SPEED: 2.5,      // No activity
} as const;

interface StatusIndicatorProps {
  status?: TorrentState;
  showLabel?: boolean;
  speed?: number; // bytes per second
}

// Calculate pulse duration based on speed (bytes/sec)
// Faster speed = shorter duration (faster pulse)
function getPulseDuration(speed: number): number {
  if (speed <= 0) return PULSE_DURATIONS.NO_SPEED;

  const kbps = speed / 1024;

  if (kbps < SPEED_THRESHOLDS.VERY_SLOW) return PULSE_DURATIONS.VERY_SLOW;
  if (kbps < SPEED_THRESHOLDS.SLOW) return PULSE_DURATIONS.SLOW;
  if (kbps < SPEED_THRESHOLDS.MEDIUM) return PULSE_DURATIONS.MEDIUM;
  if (kbps < SPEED_THRESHOLDS.FAST) return PULSE_DURATIONS.FAST;
  return PULSE_DURATIONS.VERY_FAST;
}

export function StatusIndicator({
  status = 'downloading',
  showLabel = true,
  speed = 0,
}: StatusIndicatorProps) {
  const pulseRef = React.useRef<HTMLDivElement>(null);

  // Get the color from the existing utility function
  const stateColor = getStateColor(status);
  const stateLabel = formatState(status);

  // Determine if this state should pulse
  const isActiveTransfer = ACTIVE_TRANSFER_STATES.includes(status);
  const isBackgroundActivity = BACKGROUND_ACTIVITY_STATES.includes(status);
  const shouldPulse = isActiveTransfer || isBackgroundActivity;

  // Calculate pulse duration
  const pulseDuration = React.useMemo(() => {
    if (!shouldPulse) return 0;
    if (isBackgroundActivity) return PULSE_DURATIONS.BACKGROUND;
    return getPulseDuration(speed);
  }, [shouldPulse, isBackgroundActivity, speed]);

  React.useEffect(() => {
    if (!pulseRef.current) return;

    // Reset to initial state
    gsap.set(pulseRef.current, { scale: 1, opacity: shouldPulse ? 1 : 0 });

    // Don't animate if not an active state
    if (!shouldPulse || pulseDuration === 0) {
      return;
    }

    // Create pulsing animation
    const animation = gsap.to(pulseRef.current, {
      scale: 2,
      opacity: 0,
      duration: pulseDuration,
      repeat: -1,
      ease: 'power2.out',
      delay: 0.1
    });

    return () => {
      animation.kill();
    };
  }, [status, shouldPulse, pulseDuration]);

  return (
    <div className="status-indicator">
      <div className="status-indicator__dot-container">
        <div
          className="status-indicator__dot"
          style={{ backgroundColor: stateColor }}
        />
        <div
          ref={pulseRef}
          className="status-indicator__pulse"
          style={{ backgroundColor: stateColor }}
        />
      </div>
      {showLabel && (
        <span className="status-indicator__label">{stateLabel}</span>
      )}
    </div>
  );
}

export default StatusIndicator;
