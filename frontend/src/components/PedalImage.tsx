// Renders a pedal thumbnail by cropping public/boss-guitar-pedals.png
// with CSS background-position/size. Geometry is the measured grid in
// pedalSprites.ts (the JSON map targets a different, larger sprite).
import { PEDAL_SPRITES, SPRITE_W, SPRITE_H, CELL_W, CELL_H } from '../lib/pedalSprites';

export interface PedalImageProps {
  id: string;
  /** Render width in px (aspect keeps 81:127 crop). */
  width?: number;
  className?: string;
  style?: React.CSSProperties;
}

export default function PedalImage({ id, width = 64, className = '', style }: PedalImageProps) {
  const s = PEDAL_SPRITES[id];
  const height = (width * CELL_H) / CELL_W;
  if (!s) {
    // Fallback: generic chip (parent shows name).
    return null;
  }
  return (
    <div
      className={`${className} pointer-events-none`}
      style={{
        width,
        height,
        backgroundImage: `url('/boss-guitar-pedals.png')`,
        backgroundPosition: `-${s.x}px -${s.y}px`,
        backgroundSize: `${SPRITE_W}px ${SPRITE_H}px`,
        backgroundRepeat: 'no-repeat',
        ...style,
      }}
      aria-label={s.name}
    />
  );
}
