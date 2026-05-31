require 'json'

package = JSON.parse(File.read(File.join(__dir__, '..', 'package.json')))

Pod::Spec.new do |s|
  s.name           = 'RidiRouter'
  s.version        = package['version']
  s.summary        = 'Ridi Router Expo local module'
  s.description    = 'Expo local module glue for ridi-router-mobile Rust routing facade.'
  s.author         = 'Ridi'
  s.homepage       = 'https://example.com'
  s.license        = 'MIT'
  s.platforms      = { :ios => '15.1' }
  s.source         = { :path => '.' }
  s.source_files   = '**/*.{h,m,mm,swift}'
  s.swift_version  = '5.9'
  s.dependency 'ExpoModulesCore'
  s.vendored_frameworks = 'rust/RidiRouterMobile.xcframework'
end
