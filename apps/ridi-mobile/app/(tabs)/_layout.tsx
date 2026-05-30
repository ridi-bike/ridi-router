import { Text } from 'react-native';
import { Tabs } from 'expo-router';
import { useCSSVariable } from 'uniwind';

export default function TabLayout() {
  const [background, border] = useCSSVariable([
    '--color-background',
    '--color-border',
  ]);

  return (
    <Tabs
      initialRouteName="index"
      screenOptions={{
        headerShown: false,
        sceneStyle: { backgroundColor: String(background) },
        tabBarStyle: {
          position: 'absolute',
          backgroundColor: String(background),
          borderTopColor: String(border),
        },
        tabBarIconStyle: {
          display: 'none',
        },
        tabBarItemStyle: {
          justifyContent: 'center',
        },
        tabBarActiveBackgroundColor: String(background),
        tabBarInactiveBackgroundColor: String(background),
        tabBarShowLabel: true,
      }}
    >
      <Tabs.Screen
        name="index"
        options={{
          title: 'Ride',
          tabBarLabel: ({ focused }) => (
            <Text className={focused ? 'text-primary text-xs font-semibold' : 'text-muted text-xs font-semibold'}>
              Ride
            </Text>
          ),
          tabBarAccessibilityLabel: 'Ride tab',
        }}
      />
      <Tabs.Screen
        name="data"
        options={{
          title: 'Data',
          tabBarLabel: ({ focused }) => (
            <Text className={focused ? 'text-primary text-xs font-semibold' : 'text-muted text-xs font-semibold'}>
              Data
            </Text>
          ),
          tabBarAccessibilityLabel: 'Data tab',
        }}
      />
      <Tabs.Screen
        name="settings"
        options={{
          title: 'Settings',
          tabBarLabel: ({ focused }) => (
            <Text className={focused ? 'text-primary text-xs font-semibold' : 'text-muted text-xs font-semibold'}>
              Settings
            </Text>
          ),
          tabBarAccessibilityLabel: 'Settings tab',
        }}
      />
    </Tabs>
  );
}
