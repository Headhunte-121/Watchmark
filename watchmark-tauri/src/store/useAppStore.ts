import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../utils/logger';
import type { HistoryPage, HistorySnapshot } from '../components/history/historyTypes';

/** Mirrors `network::Connectivity` in the Rust backend. */
export type Connectivity = 'unknown' | 'online' | 'tmdb_blocked' | 'offline';

export interface ConnectivityReport {
  status: Connectivity;
  internet: boolean;
  tmdb: boolean;
  checked_at: number;
}

interface LibraryPreference {
  sortBy: string;
  hideCompleted: boolean;
  scrollPos: number;
}

interface AppState {
  isCinemaMode: boolean;
  initialized: boolean;
  isApiAuthorized: boolean;
  /** True while TMDB can't be used (offline, or blocked on this network). */
  isOffline: boolean;
  connectivity: Connectivity;
  searchQuery: string;
  libraryPreferences: {
    TV: LibraryPreference;
    Movie: LibraryPreference;
  };
  /** Bumped whenever watch history may have changed outside the History tab. */
  historyVersion: number;
  /** Last History tab view, restored when the user returns to the tab. */
  historySnapshot: (HistorySnapshot & { version: number }) | null;
  /** First page fetched at boot so the tab opens instantly. */
  historyPrefetch: { page: HistoryPage; version: number } | null;
  bumpHistoryVersion: () => void;
  setHistorySnapshot: (snapshot: HistorySnapshot | null) => void;
  setSearchQuery: (query: string) => void;
  setLibraryPreference: (type: 'TV' | 'Movie', key: keyof LibraryPreference, value: any) => void;
  setCinemaMode: (mode: boolean) => Promise<void>;
  setApiAuthorized: (authorized: boolean) => void;
  setOffline: (offline: boolean) => void;
  setConnectivity: (status: Connectivity) => void;
  initializeSettings: () => Promise<void>;
  prefetchHistory: () => Promise<void>;
}

export const useAppStore = create<AppState>()(
  persist(
    (set, get) => ({
  isCinemaMode: true,
  initialized: false,
  isApiAuthorized: true,
  isOffline: false,
  connectivity: 'unknown',
  searchQuery: "",
  historyVersion: 0,
  historySnapshot: null,
  historyPrefetch: null,
  libraryPreferences: {
    TV: { sortBy: "Recently Added", hideCompleted: false, scrollPos: 0 },
    Movie: { sortBy: "Recently Added", hideCompleted: false, scrollPos: 0 },
  },
  bumpHistoryVersion: () => set((state) => ({ historyVersion: state.historyVersion + 1 })),
  setHistorySnapshot: (snapshot) =>
    set((state) => ({ historySnapshot: snapshot ? { ...snapshot, version: state.historyVersion } : null })),
  setSearchQuery: (query: string) => set({ searchQuery: query }),
  setLibraryPreference: (type, key, value) => set((state) => ({
    libraryPreferences: {
      ...state.libraryPreferences,
      [type]: {
        ...state.libraryPreferences[type],
        [key]: value
      }
    }
  })),
  setApiAuthorized: (authorized: boolean) => set({ isApiAuthorized: authorized }),
  setOffline: (offline: boolean) => set({ isOffline: offline }),
  setConnectivity: (status) =>
    set({ connectivity: status, isOffline: status === 'tmdb_blocked' || status === 'offline' }),
  setCinemaMode: async (mode: boolean) => {
    set({ isCinemaMode: mode });
    // Note: We intentionally do not auto-save to the backend here.
    // The SettingsView component handles batching and saving via the user's explicit action.
  },
  prefetchHistory: async () => {
    const version = get().historyVersion;
    try {
      const page = await invoke<HistoryPage>('fetch_history', { requestId: crypto.randomUUID(), cursor: null, pageSize: 40, mediaType: null });
      if (page && Array.isArray(page.groups) && get().historyVersion === version) {
        set({ historyPrefetch: { page, version } });
      }
    } catch (e) {
      logger.error("Failed to prefetch watch history", e);
    }
  },
  initializeSettings: async () => {
    if (get().initialized) return;
    try {
      const settings: any = await invoke('get_settings');
      logger.settings("Loaded user settings from backend.");
      // If the backend has a value, use it. If not, fallback to true but check reduced motion.
      let mode = true;
      if (settings && typeof settings.cinema_mode === 'boolean') {
        mode = settings.cinema_mode;
      } else {
        const prefersReducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
        mode = !prefersReducedMotion;
        // Optionally save this back if it's the first run
      }
      set({ isCinemaMode: mode, initialized: true });
    } catch (e) {
      logger.error("Failed to initialize settings", e);
      set({ initialized: true });
    }
  }
}),
    {
      name: 'app-storage',
      partialize: (state) => ({ libraryPreferences: state.libraryPreferences }),
    }
  )
);
