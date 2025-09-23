## Privy Example

This package demonstrates how to use Privy authentication and click-to-sign flow with Sovereign SDK rollups.

### Prerequisites

1. Install nodejs and npm
2. Create a Privy account at [https://privy.io](https://privy.io) and get your App ID
3. Copy `.env.example` to `.env` and add your configuration:
   ```
   VITE_PRIVY_APP_ID=your_privy_app_id_here
   VITE_ROLLUP_URL=http://localhost:12346
   ```

### Running the Example

1. Install dependencies: `npm install`
2. In root of the repo build & run your rollup: `cargo run`
3. In this directory run: `npm run dev`

### Features

- Email and wallet authentication via Privy
- Embedded wallet creation for email users
- Interactive transaction JSON editor
- Real-time transaction status and events display
