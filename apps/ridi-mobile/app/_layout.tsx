import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { View } from 'react-native';
import { TinyBaseProvider } from '@/tinybase/TinyBaseProvider';
import { RmdfDownloadProvider } from '@/rmdf/RmdfDownloadProvider';
import { RouteGenerationBanner, RouteGenerationProvider } from '@/routes/RouteGenerationProvider';

import '../global.css';

export default function RootLayout() {
  return (
    <TinyBaseProvider>
      <RouteGenerationProvider>
        <RmdfDownloadProvider>
          <View className="flex-1">
            <Stack screenOptions={{ headerShown: false }}>
              <Stack.Screen name="(tabs)" />
              <Stack.Screen name="rmdf-download" />
            </Stack>
            <RouteGenerationBanner />
          </View>
          <StatusBar style="auto" />
        </RmdfDownloadProvider>
      </RouteGenerationProvider>
    </TinyBaseProvider>
  );
}
