import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { TinyBaseProvider } from '@/tinybase/TinyBaseProvider';
import { RmdfDownloadProvider } from '@/rmdf/RmdfDownloadProvider';

import '../global.css';

export default function RootLayout() {
  return (
    <TinyBaseProvider>
      <RmdfDownloadProvider>
        <Stack screenOptions={{ headerShown: false }}>
          <Stack.Screen name="(tabs)" />
          <Stack.Screen name="rmdf-download" />
        </Stack>
        <StatusBar style="auto" />
      </RmdfDownloadProvider>
    </TinyBaseProvider>
  );
}
