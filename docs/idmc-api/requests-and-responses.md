<!-- source: REST_API_requests_and_responses.html -->
# REST API requests and responses

# REST API requests and responses

When you make a REST API request, the REST API returns an HTTP response code.

The following table describes the responses to the REST API requests:

| REST API Request | Successful Response | Failure Response |
| --- | --- | --- |
| GET | For an information request, returns the requested object or an array of objects when applicable. For an action request, returns the HTTP 200 success code. Can also return the REST API success object. | HTTP 403 error, including a REST API error object. |
| POST | The object that you created or updated. Can also return the HTTP 200 or 201 success codes. | HTTP 403 error, including a REST API error object. |
| PUT | The object that you updated. Can also return the HTTP 201 success code. | HTTP 403 error, including a REST API error object. |
| DELETE | The object that you deleted. Can also return the HTTP 201 success code. | HTTP 403 error, including a REST API error object. |
| PATCH | The object that you updated. Can also return the HTTP 201 success code. | HTTP 403 error, including a REST API error object. |