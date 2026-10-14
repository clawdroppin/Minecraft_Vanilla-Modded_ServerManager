/** Tag colours a server can wear (tab stripe, sidebar dot, cards). */
export const SERVER_COLORS: Record<string, { label: string; hex: string }> = {
  grass: { label: 'Grass', hex: '#22c55e' },
  diamond: { label: 'Diamond', hex: '#2dd4bf' },
  gold: { label: 'Gold', hex: '#fbbf24' },
  redstone: { label: 'Redstone', hex: '#ef4444' },
  lapis: { label: 'Lapis', hex: '#3b82f6' },
  amethyst: { label: 'Amethyst', hex: '#a78bfa' },
  copper: { label: 'Copper', hex: '#f97316' },
  stone: { label: 'Stone', hex: '#a8a29e' },
};

export function serverColor(c: string | undefined) {
  return SERVER_COLORS[c ?? 'grass']?.hex ?? SERVER_COLORS.grass.hex;
}
