import { fontFamily } from "tailwindcss/defaultTheme";
import type { Config } from "tailwindcss";

const config: Config = {
	darkMode: ["class"],
	content: ["./src/**/*.{html,js,svelte,ts}", "./node_modules/layerchart/**/*.{svelte,js}"],
	safelist: ["dark"],
	theme: {
		container: {
			center: true,
			padding: "2rem",
			screens: {
				"2xl": "1400px"
			}
		},
		extend: {
			colors: {
				border: "hsl(var(--border) / <alpha-value>)",
				input: "hsl(var(--input) / <alpha-value>)",
				ring: "hsl(var(--ring) / <alpha-value>)",
				background: "hsl(var(--background) / <alpha-value>)",
				foreground: "hsl(var(--foreground) / <alpha-value>)",
				primary: {
					DEFAULT: "hsl(var(--primary) / <alpha-value>)",
					foreground: "hsl(var(--primary-foreground) / <alpha-value>)",
					gradient: 'linear-gradient(135deg, #52b788 0%, #06ffa5 100%)',
				},
				secondary: {
					DEFAULT: "hsl(var(--secondary) / <alpha-value>)",
					foreground: "hsl(var(--secondary-foreground) / <alpha-value>)"
				},
				destructive: {
					DEFAULT: "hsl(var(--destructive) / <alpha-value>)",
					foreground: "hsl(var(--destructive-foreground) / <alpha-value>)"
				},
				muted: {
					DEFAULT: "hsl(var(--muted) / <alpha-value>)",
					foreground: "hsl(var(--muted-foreground) / <alpha-value>)"
				},
				accent: {
					DEFAULT: "hsl(var(--accent) / <alpha-value>)",
					foreground: "hsl(var(--accent-foreground) / <alpha-value>)",
					cyan: '#22d3ee',
					purple: '#a855f7',
					amber: '#fbbf24',
				},
				popover: {
					DEFAULT: "hsl(var(--popover) / <alpha-value>)",
					foreground: "hsl(var(--popover-foreground) / <alpha-value>)"
				},
				card: {
					DEFAULT: "hsl(var(--card) / <alpha-value>)",
					foreground: "hsl(var(--card-foreground) / <alpha-value>)"
				},
				glow: {
					green: 'rgba(82, 183, 136, 0.5)',
					cyan: 'rgba(34, 211, 238, 0.5)',
					purple: 'rgba(168, 85, 247, 0.5)',
				}
			},
			backdropBlur: {
				xs: '2px',
				'3xl': '40px',
			},
			boxShadow: {
				'low': '0 2px 8px rgba(0, 0, 0, 0.1)',
				'medium': '0 4px 16px rgba(0, 0, 0, 0.15)',
				'high': '0 8px 32px rgba(0, 0, 0, 0.2)',
				'float': '0 16px 48px rgba(0, 0, 0, 0.3)',
				'glow-green': '0 0 20px rgba(82, 183, 136, 0.5)',
				'glow-cyan': '0 0 20px rgba(34, 211, 238, 0.5)',
				'glow-purple': '0 0 20px rgba(168, 85, 247, 0.5)',
			},
			animation: {
				'gradient': 'gradientFlow 15s ease infinite',
				'shimmer': 'shimmer 2s infinite',
			},
			keyframes: {
				gradientFlow: {
					'0%, 100%': { backgroundPosition: '0% 50%' },
					'50%': { backgroundPosition: '100% 50%' },
				},
				shimmer: {
					'0%': { backgroundPosition: '-200% 0' },
					'100%': { backgroundPosition: '200% 0' },
				},
			},
			borderRadius: {
				lg: "var(--radius)",
				md: "calc(var(--radius) - 2px)",
				sm: "calc(var(--radius) - 4px)"
			},
			fontFamily: {
				sans: [...fontFamily.sans]
			}
		}
	},
};

export default config;
