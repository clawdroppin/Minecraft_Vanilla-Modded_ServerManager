import { Activity, Archive, Puzzle, CalendarClock, Coffee, FileCode2, Globe2, HardDrive, Layers, LayoutDashboard, Router, ShieldCheck, SlidersHorizontal, Terminal, Users } from 'lucide-react';
import type { ViewId } from '@/store/app';

export const VIEWS: { id: ViewId; label: string; icon: typeof Activity; group: 'Monitor' | 'World' | 'Configure' | 'Maintain' }[] = [
  { id: 'overview', label: 'Overview', icon: LayoutDashboard, group: 'Monitor' },
  { id: 'performance', label: 'Performance', icon: Activity, group: 'Monitor' },
  { id: 'console', label: 'Console', icon: Terminal, group: 'Monitor' },
  { id: 'players', label: 'Players', icon: Users, group: 'Monitor' },
  { id: 'worlds', label: 'Worlds', icon: Globe2, group: 'World' },
  { id: 'mods', label: 'Mods & Plugins', icon: Puzzle, group: 'World' },
  { id: 'backups', label: 'Backups', icon: Archive, group: 'World' },
  { id: 'settings', label: 'Server Settings', icon: SlidersHorizontal, group: 'Configure' },
  { id: 'software', label: 'Version & Software', icon: Layers, group: 'Configure' },
  { id: 'java', label: 'Java & Memory', icon: Coffee, group: 'Configure' },
  { id: 'automation', label: 'Automation', icon: CalendarClock, group: 'Configure' },
  { id: 'config', label: 'Config Files', icon: FileCode2, group: 'Configure' },
  { id: 'ports', label: 'Port Forwarding', icon: Router, group: 'Maintain' },
  { id: 'health', label: 'Health', icon: ShieldCheck, group: 'Maintain' },
  { id: 'files', label: 'Files & Logs', icon: HardDrive, group: 'Maintain' },
];
