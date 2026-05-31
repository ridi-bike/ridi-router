import { router } from 'expo-router';
import { Pressable, Text, View } from 'react-native';

export default function SettingsScreen() {
  return (
    <View className="flex-1 gap-4 border-border bg-background px-5 pt-16">
      <Text className="text-foreground text-2xl font-bold">Settings</Text>
      <Pressable
        accessibilityRole="button"
        className="rounded-3xl border border-border bg-card px-5 py-4 shadow-sm"
        onPress={() => router.push('/rmdf-download' as never)}
      >
        <Text className="text-foreground text-base font-semibold">Routing data download</Text>
        <Text className="text-muted mt-1 text-sm">Download or delete offline RMDF routing tiles.</Text>
      </Pressable>
    </View>
  );
}
