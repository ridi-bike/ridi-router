import { generateRouteJson } from '../../modules/ridi-router/src';
import { getRmdfNativeDirectoryPath } from '@/rmdf/storage';
import { useSyncedStore } from '@/tinybase/hooks';
import { createContext, useCallback, useContext, useMemo, useRef, useState, type PropsWithChildren } from 'react';
import { Text, View } from 'react-native';

import { DEFAULT_ROUTE_RULE_PRESET_ID } from './rulePresets';
import { persistRouteGeneration, updateSelectedRouteIndex } from './persistence';
import { normalizeRouteResult, stringifyRouteRequest } from './requests';
import type { RouteDraft, RoutePlanRow } from './types';

export type MapRouteMode =
  | { kind: 'idle' }
  | { kind: 'point_selected'; selectedPoint: unknown }
  | { kind: 'planning'; draft: RouteDraft }
  | { kind: 'generating'; draft: Required<RouteDraft>; planId?: string }
  | { kind: 'route_review'; planId: string; selectedRouteIndex: number };

type WarningBanner = { id: number; message: string } | null;

type RouteGenerationContextValue = {
  draft: RouteDraft;
  setDraft: (draft: RouteDraft) => void;
  updateDraft: (patch: Partial<RouteDraft>) => void;
  clearDraftPoint: (point: 'start' | 'finish') => void;
  isGenerating: boolean;
  selectedPlanId: string | null;
  selectedRouteIndex: number;
  selectPlan: (planId: string, selectedRouteIndex?: number) => void;
  closeSelectedPlan: () => void;
  setSelectedRouteIndex: (index: number) => void;
  generate: (draft: Required<RouteDraft>) => Promise<RoutePlanRow | null>;
  showWarning: (message: string) => void;
  warning: WarningBanner;
};

const RouteGenerationContext = createContext<RouteGenerationContextValue | null>(null);

export function RouteGenerationProvider({ children }: PropsWithChildren) {
  const store = useSyncedStore();
  const [draft, setDraftState] = useState<RouteDraft>({ rulePresetId: DEFAULT_ROUTE_RULE_PRESET_ID });
  const [isGenerating, setIsGenerating] = useState(false);
  const isGeneratingRef = useRef(false);
  const [selectedPlanId, setSelectedPlanId] = useState<string | null>(null);
  const [selectedRouteIndexState, setSelectedRouteIndexState] = useState(0);
  const [warning, setWarning] = useState<WarningBanner>(null);
  const warningCounterRef = useRef(0);

  const showWarning = useCallback((message: string) => {
    const id = warningCounterRef.current + 1;
    warningCounterRef.current = id;
    setWarning({ id, message });
    setTimeout(() => {
      setWarning((current) => (current?.id === id ? null : current));
    }, 3500);
  }, []);

  const setDraft = useCallback((nextDraft: RouteDraft) => setDraftState({ ...nextDraft, rulePresetId: nextDraft.rulePresetId ?? DEFAULT_ROUTE_RULE_PRESET_ID }), []);
  const updateDraft = useCallback((patch: Partial<RouteDraft>) => setDraftState((current) => ({ ...current, ...patch })), []);
  const clearDraftPoint = useCallback((point: 'start' | 'finish') => {
    setDraftState((current) => ({ ...current, [point]: undefined }));
  }, []);

  const selectPlan = useCallback((planId: string, selectedRouteIndex = 0) => {
    setSelectedPlanId(planId);
    setSelectedRouteIndexState(selectedRouteIndex);
    updateSelectedRouteIndex(store, planId, selectedRouteIndex);
  }, [store]);

  const closeSelectedPlan = useCallback(() => {
    if (isGeneratingRef.current) return;
    setSelectedPlanId(null);
    setSelectedRouteIndexState(0);
  }, []);

  const setSelectedRouteIndex = useCallback((index: number) => {
    setSelectedRouteIndexState(index);
    if (selectedPlanId) updateSelectedRouteIndex(store, selectedPlanId, index);
  }, [selectedPlanId, store]);

  const generate = useCallback(async (completeDraft: Required<RouteDraft>) => {
    if (isGeneratingRef.current) return null;
    isGeneratingRef.current = true;
    setIsGenerating(true);
    setDraftState(completeDraft);

    try {
      const result = await generateRouteJson(getRmdfNativeDirectoryPath(), stringifyRouteRequest(completeDraft));
      if (result.ok) {
        const routes = normalizeRouteResult(result.value);
        const plan = persistRouteGeneration({ store, draft: completeDraft, routes, status: 'succeeded' });
        setSelectedPlanId(plan.plan_id);
        setSelectedRouteIndexState(0);
        return plan;
      }

      const plan = persistRouteGeneration({
        store,
        draft: completeDraft,
        routes: [],
        status: 'failed',
        error: {
          code: String(result.error.code ?? 'route_generation_failed'),
          message: String(result.error.message ?? 'Route generation failed'),
          details: result.error.details === undefined ? undefined : String(result.error.details),
        },
      });
      setSelectedPlanId(plan.plan_id);
      setSelectedRouteIndexState(0);
      return plan;
    } catch (error) {
      const plan = persistRouteGeneration({
        store,
        draft: completeDraft,
        routes: [],
        status: 'failed',
        error: { code: 'route_generation_failed', message: error instanceof Error ? error.message : 'Route generation failed', details: String(error) },
      });
      setSelectedPlanId(plan.plan_id);
      setSelectedRouteIndexState(0);
      return plan;
    } finally {
      isGeneratingRef.current = false;
      setIsGenerating(false);
    }
  }, [store]);

  const value = useMemo<RouteGenerationContextValue>(() => ({
    draft,
    setDraft,
    updateDraft,
    clearDraftPoint,
    isGenerating,
    selectedPlanId,
    selectedRouteIndex: selectedRouteIndexState,
    selectPlan,
    closeSelectedPlan,
    setSelectedRouteIndex,
    generate,
    showWarning,
    warning,
  }), [draft, setDraft, updateDraft, clearDraftPoint, isGenerating, selectedPlanId, selectedRouteIndexState, selectPlan, closeSelectedPlan, setSelectedRouteIndex, generate, showWarning, warning]);

  return <RouteGenerationContext.Provider value={value}>{children}</RouteGenerationContext.Provider>;
}

export function useRouteGeneration() {
  const context = useContext(RouteGenerationContext);
  if (!context) throw new Error('useRouteGeneration must be used within RouteGenerationProvider.');
  return context;
}

export function RouteGenerationBanner() {
  const { isGenerating, warning } = useRouteGeneration();
  if (!isGenerating && !warning) return null;
  return (
    <View pointerEvents="none" className="absolute left-4 right-4 top-14 z-50 gap-2">
      {isGenerating ? (
        <View className="rounded-2xl border border-primary/30 bg-primary px-4 py-3 shadow-lg">
          <Text className="text-center text-base font-semibold text-primary-foreground">Generating routes…</Text>
        </View>
      ) : null}
      {warning ? (
        <View className="rounded-2xl border border-yellow-500/30 bg-yellow-100 px-4 py-3 shadow-lg">
          <Text className="text-center text-sm font-semibold text-yellow-950">{warning.message}</Text>
        </View>
      ) : null}
    </View>
  );
}
