import { useRouter } from 'expo-router';
import { Pressable, ScrollView, Text, View } from 'react-native';

import { formatDistance } from '@/routes/format';
import { softDeletePlan } from '@/routes/persistence';
import { useRouteGeneration } from '@/routes/RouteGenerationProvider';
import { useSyncedPlans, useSyncedStore } from '@/tinybase/hooks';

export default function DataScreen() {
  const plans = useSyncedPlans();
  const store = useSyncedStore();
  const router = useRouter();
  const { selectPlan } = useRouteGeneration();

  return (
    <View className="flex-1 bg-background px-4 pb-28 pt-16">
      <Text className="text-3xl font-semibold text-foreground">Saved routes</Text>
      <Text className="mt-2 text-base text-muted">Generated route plans persist here.</Text>

      {plans.length === 0 ? (
        <View className="mt-8 rounded-3xl border border-border bg-card p-5">
          <Text className="text-lg font-semibold text-foreground">No saved route plans yet</Text>
          <Text className="mt-2 text-muted">Tap a map point, choose Ride to, then generate a route.</Text>
        </View>
      ) : (
        <ScrollView className="mt-6" contentContainerStyle={{ gap: 12, paddingBottom: 24 }} showsVerticalScrollIndicator={false}>
          {plans.map((plan) => (
            <Pressable
              key={plan.plan_id}
              accessibilityRole="button"
              className="rounded-3xl border border-border bg-card p-5 active:opacity-80"
              onPress={() => {
                selectPlan(plan.plan_id, Number(plan.selected_route_index ?? 0));
                router.navigate('/(tabs)');
              }}
            >
              <View className="flex-row items-start justify-between gap-3">
                <View className="flex-1">
                  <Text className="text-lg font-semibold text-foreground" numberOfLines={1}>{plan.start_label}</Text>
                  <Text className="mt-1 text-lg font-semibold text-foreground" numberOfLines={1}>→ {plan.finish_label}</Text>
                </View>
                <Text className={plan.generation_status === 'failed' ? 'text-sm font-semibold text-red-500' : 'text-sm font-semibold text-primary'}>
                  {plan.generation_status}
                </Text>
              </View>

              <View className="mt-4 gap-1">
                <Text className="text-sm text-muted">Rule: {plan.rule_label}</Text>
                <Text className="text-sm text-muted">Routes: {plan.route_count} · Distance: {formatDistance(plan.total_distance_m)}</Text>
                <Text className="text-sm text-muted">Created: {new Date(plan.created_at).toLocaleString()}</Text>
                {plan.error_message ? <Text className="text-sm text-red-500">{plan.error_message}</Text> : null}
              </View>

              <Pressable
                accessibilityRole="button"
                className="mt-4 self-start rounded-full bg-background px-4 py-2"
                onPress={(event) => {
                  event.stopPropagation();
                  softDeletePlan(store, plan.plan_id);
                }}
              >
                <Text className="font-semibold text-red-500">Delete</Text>
              </Pressable>
            </Pressable>
          ))}
        </ScrollView>
      )}
    </View>
  );
}
