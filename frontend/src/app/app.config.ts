import {
  provideHttpClient,
  withFetch,
  withInterceptors,
  withInterceptorsFromDi,
} from '@angular/common/http';
import { ApplicationConfig, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideRouter, withComponentInputBinding } from '@angular/router';
import { provideOAuthClient } from 'angular-oauth2-oidc';
import { routes } from './app.routes';
import { reauthOn401 } from './core/auth.interceptor';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes, withComponentInputBinding()),
    // The OAuth interceptor (class-based) adds the access token to /api/ requests once
    // signed in; without sign-in (auth mode `none`) there is no token and nothing is added.
    provideHttpClient(withFetch(), withInterceptors([reauthOn401]), withInterceptorsFromDi()),
    provideOAuthClient({ resourceServer: { allowedUrls: ['/api/'], sendAccessToken: true } }),
  ],
};
