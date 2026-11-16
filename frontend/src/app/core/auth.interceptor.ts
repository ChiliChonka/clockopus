import { HttpErrorResponse, HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { catchError, throwError } from 'rxjs';
import { AuthService } from './auth.service';

/**
 * A `401` from the API while sign-in is enabled means the session ended (e.g. the refresh token
 * expired): start a new sign-in instead of showing an error. The access token itself is added by
 * angular-oauth2-oidc (`provideOAuthClient`, resource server `/api/`).
 */
export const reauthOn401: HttpInterceptorFn = (req, next) => {
  const auth = inject(AuthService);
  return next(req).pipe(
    catchError((e: unknown) => {
      if (
        e instanceof HttpErrorResponse &&
        e.status === 401 &&
        auth.enabled() &&
        req.url.startsWith('/api/')
      ) {
        auth.login();
      }
      return throwError(() => e);
    }),
  );
};
