<!-- source: Search_API.html -->
# Search API

# Search API

Use the Search API to search for records that match your search criteria. You can search across all the business entities or within a specific business entity, in English or Japanese. You can filter the search results based on severity levels and validation statuses. You can also filter search results based on fields that are blank or contain values.

The API uses the search resource.

To search for records, submit a POST request with the following URI:

<baseApiUrl>/search/public/api/v1/search

**Note:** 

After you edit record and field privileges or data access rules, users with custom user roles must wait at least 10 minutes before they search for records using Search REST API. If users search for records immediately after the changes, the API returns inaccurate results.

When you search for records with multiple conditions on fields in the same field group or nested field group, the Search API evaluates the conditions within each entry. The API returns a record only if a single entry matches all the conditions.

For example, a record contains home and office address entries in the Address field group. The home address has the city New York and the street Main Street. The office address has the city Chicago and the street Oak Avenue.

If you search for the record with the condition city = Chicago and street = Main Street, the search doesnât return the record because no single address entry matches both conditions.

To search for records with multiple conditions based on the fields of a field group, ensure that you reindex the records.

**Note:** 

Effective in the October 2026 release, the ability to search for records with multiple conditions within a single field group entry is available for preview.

Preview functionality is supported for evaluation purposes but is unwarranted and is not supported in production environments or any environment that you plan to push to production. Informatica intends to include the preview functionality in an upcoming release for production use, but might choose not to in accordance with changing market or technical circumstances. For more information, contact Informatica Global Customer Support.

## Request body

You can use the following parameters in the search request body:

**Note:** 

Use either the search or fields parameter, but not both.

| Parameter | Type | Description |
| --- | --- | --- |
| search | String | Search string.  To retrieve all the records, use an asterisk (\*).  To perform a fuzzy search, enter a search word or phrase. When you search for records, you can use additional wildcard characters, such as \*, ~, &&, and ||.  You can use search strings similar to the ones in the following examples:   * John Smith. Searches for records that contain John Smith as a field value. * Jo\*n. Searches for records that start with Jo and end with n, such as Johansson or Jordan. * Hans && Williams. Searches for records that contain Hans and Williams in the same record. For example, the first name is Hans, and the last name is Williams.   To perform an exact search, enclose the search string within double quotes in the following format:  "\"<Field\_Value>\""  For example, to search for records that contain the field value London, specify the value in the following format:  "\"London\""  Use this format to perform exact searches for string, date, and date and time field types. To perform an exact search on records based on other field types, don't enclose the value in the specified format.  **Note:**  When you use only white spaces as a search string without any other characters, the search doesn't return any results.  To search for records by specifying a date, ensure that you use the YYYY-MM-DD format.  When you search for records with keywords, such as AND, OR, and NOT, as field values, ensure that you specify the values in the following format:   * "\"OR\"" * "\"AND\"" * "\"NOT\""   If you don't enter the values in the specified format, the Search REST API doesn't return any response.  However, if these keywords are part of a field value, such as SAND AND MARVEL, you don't have to enclose the keyword part of the value in the specified format for the Search REST API to return the response.  You can't search for records with similar values in system fields. However, you can search for records that exactly match the specified values in system fields and continue to use the supported wildcards in search strings for system fields.  For more information about these wildcard characters, see Using the search box to find records in the Customer 360 help.  **Note:**  Informatica recommends that you enter a search string that doesn't exceed 4,000 characters.  If the recommended character limit exceeds, the search request might fail intermittently. |
| fields | String | Searches records for specific basic, smart, system, and dynamic fields. For example , searching "J\*" for the first name field returns all names starting with "J". |
| entityType | String | Internal ID of the business entity you want to search. |
| filters | Array | Specifies a list of fields that you want to use to filter the search results.  When you search for records with multiple conditions on fields in the same field group or nested field group, the Search API evaluates the conditions within each record. The API returns a record only if a single entry matches all the conditions.  To search for records with multiple conditions within a single field group entry, ensure that you reindex the records. |
| filter > operator | String | The Boolean operator that joins multiple filters that you specify. Use AND or OR as the Boolean operator. |
| filters > filter > comparator | String | Condition that specifies how to filter search results.  Use one of the following comparators:   * EQUALS. Returns records that match the specified field value.  For example, you can filter the employees whose department is equal to product management. * NOT\_EQUALS. Returns records that don't match the specified value.  **Note:**  When you use the NOT\_EQUALS filter comparator in the request, the response doesn't return records based on fields that contain blank or null values.  To search for records with fields that contain blank or null values, use the IS\_MISSING filter comparator in the request.  For example, you can filter employees whose country is not equal to India. * IN. Returns records with field values that match any of the specified values.  For example, you can filter employees who belong to cities such as Chennai, Bangalore, and Hyderabad. * BETWEEN. Returns records that contain field values within the specified range.  For example, you can filter people aged between 20 and 30. * GREATER\_THAN. Returns records with field values greater than the specified value.  For example, you can filter employees with more than 5 years experience. * LESS\_THAN. Returns records with field values less than the specified value.  For example, you can filter employees with less than 3 years experience. * GREATER\_THAN\_EQUAL\_TO. Returns records with field values greater than or equal to the specified value.  For example, you can filter students with a score greater than or equal to 80. * LESS\_THAN\_EQUAL\_TO. Returns records with field values less than or equal to the specified value.  For example, you can filter students with a score less than or equal to 35. * STARTS\_WITH. Returns records with field values that start with a specified value.  For example, you can filter records where the first name starts with John. * CONTAINS. Returns records with field values that contain the exact words in the same sequence as specified in the filter condition.  For example, if you filter results by first names that contain the word James, the filter can return names such as James Earl, James Henry, and William James. However, the filter doesn't return Jameson. Similarly, if you filter results by first names that contain Anne Marie, the filter can return names, such as Anne Marie and Anne-Marie. However, the filter doesn't return Marie Anne because the order of words doesn't match. * IS\_MISSING. Returns records that don't contain values in specific fields.  For example, you can filter records that don't contain values in the middle name field. * IS\_NOT\_MISSING. Returns records that contain values in specific fields.  For example, you can filter records that contain values for the country field.   **Note:**  If you want to you use the IS\_MISSING and IS\_NOT\_MISSING filter comparators, ensure that you run the Generate Search or Report Indexes or Match Keys job at least once to reindex records. |
| filters > filter > fieldName | String | Name of the business entity field or system field that you want to use as a filter.  You can use the following system fields:   * validationType. Filters search results by error types, such as ERROR, WARNING, or INFO. You can also filter search results by internal IDs of failed rule associations. To view the internal rule ID, open the rule association. * validationState. Filters search results by the specified validation status, such as PASSED or FAILED. * \_indexValidationError.code. Filters search results to retrieve the records with index validation errors, such as records that exceed the dynamic field index limit or the field group entry index limit. |
| filters > filter > fieldValue | String | Value to use to filter the results.  **Note:**  You can use a partial word or value with the STARTS\_WITH or CONTAINS comparators.  To filter records that exceed the index limit, use the following error codes:   * INDEX.100001. For dynamic field index limit. * INDEX.100002. For field group entry index limit. |
| filters > filter > fieldValueGroups | String | Filters search results based on certain criteria that define a group.  For example, you can filter search results based on their error type groups, such as error, warning, and info. |
| pageSize | Number | Number of search results to display on each page. |
| pageOffset | Number | Optional. Number of pages to skip. For example, pageOffset=2 means that the search must skip the first two pages of the search results. Default is 0. |
| recordsToReturn | Number | Number of search results to return. |
| recordOffset | Number | Number of records to skip. For example, recordOffset=200 indicates that the search must skip the first 200 search results. Default is 0. |
| maxRecords | Number | Maximum number of records to return.  **Note:**  The search cannot return more than 10,000 records per page. |
| sort | Array | Specifies a list of fields based on which you want to sort the search results. |
| sort > fieldName | String | Name of the field by which you want to sort the search results. |
| sort > order | String | Order in which you want to sort the search results.  Use one of the following sort orders:   * ASCENDING * DESCENDING |
| searchLocale | String | Optional. Code of the language that you want to use to search for records.  Use one of the following codes:   * en * ja   Default is en.  Before you specify any value other than en, ensure that the language is configured as a searchable language in Business 360 Console. |

## Request response

The request response returns the search results.

The following table lists the response parameters:

| Parameter | Type | Description |
| --- | --- | --- |
| searchResult | Object | Contains the search results. |
| hits | Number | Number of search results. |
| records | Array | Lists the search results with their failed rule association details, based on the filter and sort criteria that you specify. |
| pageSize | Number | Number of search results on a page. |
| pageOffset | Number | Number of pages to skip. For example, pageOffset=2 means that the search must skip the first two pages of the search results. Default is 0. |
| recordsToReturn | Number | Number of search results to return. |
| recordOffset | Number | Number of records to skip. For example, recordOffset=200 indicates that the search must skip the first 200 search results. Default is 0. |
| maxRecords | Number | Maximum number of records to return.  **Note:**  The search cannot return more than 10,000 records per page. |

The POST response includes the following meta parameters:

| Parameter | Type | Description |
| --- | --- | --- |
| score | Number | The current trust score of the record. |
| businessId | String | Unique identifier of the business entity record. |
| id | String | Internal ID of the record. Do not use. |
| businessEntity | String | Name of the business entity to which the record belongs. |
| createdBy | String | Name of the person who created the record. |
| creationDate | String | Date when the record was created. |
| updatedBy | String | Name of the user who last updated the record. |
| lastUpdateDate | String | Date when the master record was last updated. |
| type | String | Internal ID of the business entity to which the record belongs. |
| status | String | Indicates whether the record is active, inactive, or in pending status.  **Note:**  This parameter is the same as the state parameter in the business entity record APIs. For more information about the state parameter, see Read Master Record by Business ID . |

## Sample requests

### Request with the search parameter

You might use the following request to find all the records of the Person business entity associated with the name Arny and the state of California:

POST https://use4-mdm.dm-us.informaticacloud.com/search/public/api/v1/search  
Content-Type: application/json  
Accept: application/json  
IDS-SESSION-ID: XXXXXXXXXXXXXXXXXXXXXX  
MODEL\_VERSION: 1583398245259  
  
{  
 "entityType":"c360.person",  
 "search":"Arny\*",  
 "filters":{  
 "filter":[  
 {  
 "comparator":"EQUALS",  
 "fieldName":"c360person.PostalAddress.state.Name",  
 "fieldValue":"California"  
 }  
 ]  
 }  
}

To use the latest search configuration, ensure that you include the model version in the search request. If you don't include the model version, the search request might use the search configuration from the metadata cache of the previous REST API requests.

For more information about model version, see [Request header](Request_header.md "Request header").

### Request with the fields parameter

You might use the following request to find all the records that start with "Jona" for the full name field:

{  
 "entityType": "c360.person",  
 "fields": {  
 "c360person.fullName": "Jona\*"  
 },  
 "filters": {  
 "filter": [  
 {  
 "comparator": "EQUALS",  
 "fieldName": "c360person.gender.Name",  
 "fieldValue": "Male"  
 }  
 ]  
 }  
}

**Note:** 

If you use filter comparators other than IS\_MISSING or IS\_NOT\_MISSING, ensure that you specify a field in the fields section.

### Request to filter search results based on validation errors

The following table lists the parameters to use to filter search results based on error type, rule ID, and validation status:

| Parameter | System Field | Search Parameter Value | Description |
| --- | --- | --- | --- |
| fieldName > fieldValueGroups | validationType | ERROR, WARNING, or INFO | Filters search results based on error types, such as ERROR, WARNING, or INFO. |
| fieldName > fieldValue | validationType | <Internal ID of rule associations> | Filters search results based on the internal IDs of failed rule associations. To view the internal rule ID, open the rule association. |
| fieldName > fieldValue | validationState | PASSED or FAILED | Filters search results based on the specified validation status. |

You might use the following request to filter records based on the error type INFO, rule ID should\_not\_contain\_number\_1687874505794, and validation status FAILED:

{  
 "recordOffset": 0,  
 "debug": true,  
 "recordsToReturn": 25,  
 "sort": [],  
 "entityType": "custombe1",  
 "search": "\*",  
 "pageSize": 25,  
 "pageOffset": 0,  
 "filters": {  
 "filter": [   
 {"comparator": "IN","fieldName": "validationType", "fieldValue": ["should\_not\_contain\_number\_1687874505794"],  
 "fieldValueGroups": ["INFO"]  
 },  
 {"comparator": "IN","fieldName": "validationState", "fieldValue": ["FAILED"]}  
 ]},  
 "facets": [  
 {  
 "facetField": "validationType",  
 "type": "SIMPLE",  
 "orderField": "KEY",  
 "order": "ASCENDING"  
 }  
 ],  
 "highlight": true,  
 "hierarchyFilters": []  
}

### Request to search for deleted master records

The following table lists the parameter to use to filter search results based on record state indicator:

| Parameter | System Field | Search Parameter Value | Description |
| --- | --- | --- | --- |
| fieldName > fieldValue | recordStateIndicator | ACTIVE or DELETED | Filters search results that are active or deleted based on the parameter value. |

You might use the following request to search for all the deleted master records of the Person business entity:

{  
 "entityType": "c360.person",  
 "search": "\*",  
 "filters":{  
 "filter":[  
   
 {  
 "comparator":"IN",  
 "fieldName": "recordStateIndicator",  
 "fieldValue": [  
 "DELETED"  
 ]  
 }  
   
 ]  
 },  
 "pageSize":25,  
 "pageOffset":0,  
 "recordOffset":0,  
 "recordsToReturn":25  
}

### Request to filter fields with or without values

You can use the IS\_MISSING and IS\_NOT\_MISSING filter comparators to search for records with fields that are blank or contain values.

Consider the following guidelines when you use the IS\_MISSING and IS\_NOT\_MISSING filter comparators:

* Don't specify a field value for the IS\_MISSING and IS\_NOT\_MISSING filter comparators.
* If you use the IS\_MISSING and IS\_NOT\_MISSING filter comparators for a field, don't use another comparator for the same field.

For example, you can't use both EQUALS and IS\_MISSING filter comparators for the first name field.

You might use the following request to find all the records for the Person business entity that don't contain any value for the Designation field and matches the specified value for the First Name field:

POST https://use4-mdm.dm-us.informaticacloud.com/search/public/api/v1/search  
Content-Type: application/json  
Accept: application/json  
IDS-SESSION-ID: XXXXXXXXXXXXXXXXXXXXXX  
MODEL\_VERSION: 1583398245259  
{  
 "recordOffset": 0,  
 "recordsToReturn": 25,  
 "sort": [],  
 "entityType": "c360.person",  
 "search": "\*",  
 "pageSize": 25,  
 "pageOffset": 0,  
 "filters": {  
 "filter": [  
 {  
 "fieldName": "c360person.designation",  
 "comparator": "IS\_MISSING"  
 },  
 {  
 "comparator": "IN",  
 "fieldName": "c360person.firstName",  
 "fieldValue": [  
 "Dav"  
 ]  
 }  
 ]  
 },  
 "highlight": true,  
 "hierarchyFilters": [],  
 "searchLocale": [  
 "en"  
 ],  
 "aggregateSelectedFacetFilterValues": false  
}

You might use the following request to find all the records of the Person business entity that contain values for the Designation field and the First Name field equals to John:

POST https://use4-mdm.dm-us.informaticacloud.com/search/public/api/v1/search  
Content-Type: application/json  
Accept: application/json  
IDS-SESSION-ID: XXXXXXXXXXXXXXXXXXXXXX  
MODEL\_VERSION: 1583398245259  
{  
 "recordOffset": 0,  
 "recordsToReturn": 25,  
 "sort": [],  
 "entityType": "c360.person",  
 "search": "\*",  
 "pageSize": 25,  
 "pageOffset": 0,  
 "filters": {  
 "filter": [  
 {  
 "fieldName": "c360person.designation",  
 "comparator": "IS\_NOT\_MISSING"  
 },  
 {  
 "comparator": "EQUALS",  
 "fieldName": "c360person.firstName",  
 "fieldValue":   
 "John"  
   
 }  
 ]  
 },  
 "highlight": true,  
 "hierarchyFilters": [],  
 "searchLocale": [  
 "en"  
 ],  
 "aggregateSelectedFacetFilterValues": false  
}

### Request to filter records updated within a specific time period

You might use the following request to find all the Person records that were updated within a specified time period:

POST https://use4-mdm.dm-us.informaticacloud.com/search/public/api/v1/search  
Content-Type: application/json  
Accept: application/json  
IDS-SESSION-ID: XXXXXXXXXXXXXXXXXXXXXX  
MODEL\_VERSION: 1583398245259  
{  
 "recordOffset": 0,  
 "recordsToReturn": 25,  
 "sort": [],  
 "entityType": "c360.person",  
 "search": "\*",  
 "pageSize": 25,  
 "pageOffset": 0,  
 "filters": {  
 "filter": [  
 {  
 "comparator": "BETWEEN",  
 "fieldName": "\_meta.lastUpdateDate",  
 "range": {  
 "from": "1719987281000",  
 "to": "1728300436000"  
 }  
 }  
 ]  
 }  
}

You can use additional \_meta parameters, such as creationDate, updatedBy, and createdBy, to filter records. For more information about \_meta parameters, see [\_meta parameters table](Business_entity_record_APIs.md#ww3_6_46_14_4_4_2 "Business entity record APIs").

### Request to search for records based on dynamic field values

You might use the following request to find all the records of the Product business entity based on dynamic field values, such as product description, quantity, and electronics display size:

POST https://use4-mdm.dm-us.informaticacloud.com/search/public/api/v1/search  
Content-Type: application/json  
Accept: application/json  
IDS-SESSION-ID: XXXXXXXXXXXXXXXXXXXXXX  
MODEL\_VERSION: 1583398245259  
{  
 "recordOffset": 0,  
 "recordsToReturn": 25,  
 "sort": [],  
 "entityType": "Product",  
 "search": "ABC",  
 "pageSize": 25,  
 "pageOffset": 0,  
 "filters": {  
 "filter": []  
 },  
 "facets": [  
 {  
 "facetField": "Product.product\_name",  
 "type": "SIMPLE",  
 "orderField": "KEY",  
 "order": "ASCENDING",  
 "searchFieldId": "Product.sConfig.product\_name"  
 },  
 {  
 "facetField": "\_meta.states.base",  
 "type": "SIMPLE",  
 "orderField": "KEY",  
 "order": "ASCENDING",  
 "searchFieldId": "entity.search.be.sConfig.\_meta.states.base"  
 },  
 {  
 "facetField": "\_cm.vader.message",  
 "type": "SIMPLE",  
 "orderField": "KEY",  
 "order": "ASCENDING",  
 "searchFieldId": "entity.search.be.sConfig.\_cm.vader.fail.rule.idWithProvider"  
 },  
 {  
 "facetField": "\_meta.states.validation",  
 "type": "SIMPLE",  
 "orderField": "KEY",  
 "order": "ASCENDING",  
 "searchFieldId": "entity.search.be.sConfig.\_meta.states.validation"  
 }  
 ],  
 "highlight": true,  
 "hierarchyFilters": [],  
 "searchLocale": [  
 "en"  
 ],  
 "aggregateSelectedFacetFilterValues": false,  
 "includeDynamicFaceting": true  
}

### Request to filter records that exceed the dynamic field index limit

You might use the following request to find the Keyboard record in the Item business entity that exceeds the dynamic field index limit:

{  
 "entityType": "Item",  
 "fields": {  
 "Product.product\_name": "Keyboard"  
 },  
 "filters": {  
 "filter": [  
 {  
 "comparator": "EQUALS",  
 "fieldName": "**\_indexValidationError.code**",  
 "fieldValue": "**INDEX.100001**"  
 }  
 ],  
 "operator": "AND"  
 },  
 "facetSize": 0,  
 "searchLocale": []  
}

For more information, see [Filtering records that exceed the index limit](Search_API.md#ww3_6_43_23_1 "Search API").

### Request to filter records that exceed the field group entry index limit

You might use the following request to find the James Patel record in the c360.person business entity that exceeds the field group entry index limit:

{  
 "entityType": "c360.person",  
 "fields": {  
 "c360person.lastName": "Patel"  
 },  
 "filters": {  
 "filter": [  
 {  
 "comparator": "EQUALS",  
 "fieldName": "**\_indexValidationError.code**",  
 "fieldValue": "**INDEX.100002**"  
 }  
 ],  
 "operator": "AND"  
 },  
 "facetSize": 0,  
 "searchLocale": []  
}

For more information, see [Filtering records that exceed the index limit](Search_API.md#ww3_6_43_23_1 "Search API").

## Sample responses

When you make a search request, the response retrieves records that match your search criteria.

### Response with the search parameter

The following search response contains the Person business entity record associated with the name Arny:

{  
 "searchResult": {  
 "hits": 1,  
 "pageSize": 25,  
 "pageOffset": 0,  
 "recordsToReturn": -1,  
 "recordOffset": -1,  
 "maxRecords": 10000,  
 "records": [  
 {  
 "c360person.Insurance": [  
 {  
 "insuranceType": {  
 "Name": "Casualty"  
 }  
 }  
 ],  
 "\_meta": {  
 "score": 13,  
 "lastUpdateDate": "1622227656589",  
 "businessId": "29924818400438832441125634728",  
 "id": "60b13ac80cabf75604b602a9",  
 "businessEntity": "c360.person",  
 "type": "c360.person",  
 "status": "ACTIVE"  
 },  
 "c360person.designation": "Assistant Professor",  
 "c360person.lastName": "Eidelburg",  
 "c360person.Phone": [  
 {  
 "phoneNumber": "016-7658"  
 }  
 ],  
 "c360person.AlternateName": [  
 {  
 "AlternateName": "Arny Eidelburg"  
 }  
 ],  
 "c360person.firstName": "Arny",  
 "c360person.gender": {  
 "Name": "Male"  
 },  
 "c360person.PostalAddress": [  
 {  
 "country": {  
 "Name": "United States"  
 },  
 "city": "San Jose",  
 "postalCode": "95111",  
 "county": "Santa Clara",  
 "addressLine1": "17 W Jackson Blvd",  
 "state": {  
 "Name": "California"  
 }  
 }  
 ],  
 "c360person.dfv\_person": {  
 "birthday": 1753783759000,  
 "size": "Small",  
 "color": "Red",  
 "alive": true,  
 "species": [  
 {  
 "color": "Blue",  
 "size": "7",  
 "gene": [  
 {  
 "mutation": 1754474996000,  
 "gene\_val": 9,  
 "gene\_sequence": "gene sequence",  
 "\_id": "b77d7d013b074042adca6e9eb53da31a"  
 }  
 ],  
 "name": "Arny",  
 "count": 5,  
 "extinct": true,  
 "\_id": "f877d857000e459c81e9695381554352",  
 "measurement": "6"  
 }  
 ],  
 "uni\_picker": {  
 "Code": "M",  
 "Name": "Male"  
 },  
 "temperature": 3,  
 "name": "Root name",  
 "radius": "4",  
 "measurement": "1",  
 "miles": 2  
 }  
 }  
 ]  
 }  
}

### Response with the fields parameter

The following search response contains all records for the full name field that start with "Jona":

{  
 "searchResult": {  
 "hits": 1,  
 "pageSize": 25,  
 "pageOffset": 0,  
 "recordsToReturn": -1,  
 "recordOffset": -1,  
 "maxRecords": 10000,  
 "records": [  
 {  
 "c360person.fullName": "Jonathon Javier Crasford",  
 "\_meta": {  
 "score": 13.0,  
 "lastUpdateDate": "1617657132604",  
 "businessId": "29840506262486653401553471970",  
 "id": "29840506262486653401553471970",  
 "businessEntity": "c360.person",  
 "type": "c360.person",  
 "status": "ACTIVE"  
 },  
 "c360person.designation": "Analyst Programmer",  
 "c360person.lastName": "Crasford",  
 "c360person.Phone": [  
 {  
 "phoneNumber": "298-9816"  
 }  
 ],  
 "c360person.AlternateName": [  
 {  
 "AlternateName": "Ferrel Vannah"  
 }  
 ],  
 "c360person.firstName": "Jonathon",  
 "c360person.gender": {  
 "Name": "Male"  
 },  
 "c360person.PostalAddress": [  
 {  
 "country": {  
 "Name": "United States"  
 },  
 "city": "Los Angeles",  
 "postalCode": "90006",  
 "county": "Los Angeles",  
 "addressLine1": "5 Tomahawk Dr",  
 "state": {  
 "Name": "California"  
 }  
 }  
 ]  
 }  
 ]  
 }  
}

### Response with search results containing validation errors

The following request contains a search result with error type INFO, rule ID should\_not\_contain\_number\_1687874505794, and validation status FAILED:

{  
 "searchResult": {  
 "hits": 1,  
 "pageSize": 25,  
 "pageOffset": 0,  
 "recordsToReturn": 25,  
 "recordOffset": 0,  
 "maxRecords": 10000,  
 "records": [  
 {  
 "\_cm": {  
 "ruleAssociation": {  
 "fail": [  
 {  
 "provider": "mdm-business-entity",  
 "rule": [  
 {  
 "id": "should\_not\_contain\_number\_1687874505794"  
 }  
 ]  
 }  
 ]  
 }  
 },  
 "\_meta": {  
 "score": 5.1987,  
 "updatedBy": "mdm-enrichment-service",  
 "createdBy": "delta\_mrel1s1\_jun27",  
 "lastUpdateDate": "1687874602219",  
 "businessId": "MDM0000000LFLS",  
 "id": "MDM0000000LFLS",  
 "businessEntity": "custombe1",  
 "type": "custombe1",  
 "creationDate": "1687874601610",  
 "states": {  
 "validation": "FAILED"  
 },  
 "status": "ACTIVE"  
 },  
 "custombe1.fname": "user12"  
 }  
 ]  
 }  
}

### Response with deleted master records

The following search response contains all the deleted master records of the Person business entity:

{  
 "searchResult": {  
 "hits": 3,  
 "pageSize": 25,  
 "pageOffset": 0,  
 "recordsToReturn": 25,  
 "recordOffset": 0,  
 "maxRecords": 10000,  
 "records": [  
 {  
 "c360person.middleName": "Hunar",  
 "\_meta": {  
 "score": 1.0,  
 "updatedBy": "mdm-org-provisioning",  
 "createdBy": "c360\_delta\_s1\_25aug",  
 "lastUpdateDate": "1715665888564",  
 "businessId": "MDM000000RE5JL",  
 "id": "MDM000000RE5JL",  
 "businessEntity": "c360.person",  
 "type": "c360.person",  
 "creationDate": "1704257316720",  
 "states": {  
 "recordStateIndicator": "DELETED",  
 "validation": "SKIPPED"  
 },  
 "status": "ACTIVE"  
 },  
 "c360person.title": "Minima.",  
 "c360person.designation": "Administrator, arts",  
 "c360person.lastName": "Lewis",  
 "c360person.firstName": "Maria 08\_10",  
 "c360person.suffixName": {  
 "Code": "III",  
 "Name": "III"  
 },  
 "c360person.gender": {  
 "Code": "M",  
 "Name": "Male"  
 },  
 "c360person.PostalAddress": [  
 {  
 "country": {  
 "Code": "GQ",  
 "Name": "Equatorial Guinea"  
 },  
 "city": "South Megan",  
 "postalCode": "18102660.4",  
 "county": "Dolore.",  
 "addressLine1": "8231 Sanchez Ranch",  
 "\_id": "31af7c32a4cf4557767b"  
 }  
 ]  
 },  
 {  
 "c360person.middleName": "Hunar",  
 "\_meta": {  
 "score": 1.0,  
 "updatedBy": "c360\_delta\_s1\_25aug",  
 "createdBy": "c360\_delta\_s1\_25aug",  
 "lastUpdateDate": "1718703319506",  
 "businessId": "MDM000000RE5JU",  
 "id": "MDM000000RE5JU",  
 "businessEntity": "c360.person",  
 "type": "c360.person",  
 "creationDate": "1704257316720",  
 "states": {  
 "recordStateIndicator": "DELETED",  
 "validation": "SKIPPED"  
 },  
 "status": "ACTIVE"  
 },  
 "c360person.title": "Minima.",  
 "c360person.designation": "Administrator, arts",  
 "c360person.lastName": "Lewis",  
 "c360person.firstName": "Maria 08\_19",  
 "c360person.suffixName": {  
 "Code": "III",  
 "Name": "III"  
 },  
 "c360person.gender": {  
 "Code": "M",  
 "Name": "Male"  
 },  
 "c360person.PostalAddress": [  
 {  
 "country": {  
 "Code": "GQ",  
 "Name": "Equatorial Guinea"  
 },  
 "city": "South Megan",  
 "postalCode": "18102660.4",  
 "county": "Dolore.",  
 "addressLine1": "8240 Sanchez Ranch",  
 "\_id": "31af7c32a4cf4557767b"  
 }  
 ]  
 },  
 {  
 "\_meta": {  
 "score": 1.0,  
 "updatedBy": "mdm-org-provisioning",  
 "createdBy": "c360\_delta\_s1\_25aug",  
 "lastUpdateDate": "1715665888933",  
 "businessId": "MDM000000S08AO",  
 "id": "MDM000000S08AO",  
 "businessEntity": "c360.person",  
 "type": "c360.person",  
 "creationDate": "1708336773195",  
 "states": {  
 "recordStateIndicator": "DELETED",  
 "validation": "SKIPPED"  
 },  
 "status": "ACTIVE"  
 },  
 "c360person.lastName": "Kumar",  
 "c360person.firstName": "manoj",  
 "c360person.PostalAddress": [  
 {  
 "country": {  
 "Code": "AL",  
 "Name": "Albania"  
 },  
 "county": "sdfddsfsd",  
 "addressLine1": "asdfsdfdsfds",  
 "\_id": "578f78ea24ef46b0968627fd452d05f0"  
 },  
 {  
 "country": {  
 "Code": "DZ",  
 "Name": "Algeria"  
 },  
 "addressLine1": "xxxxxxx",  
 "\_id": "bc31aa2db5184bf3949f145b06732c06"  
 },  
 {  
 "country": {  
 "Code": "AU",  
 "Name": "Australia"  
 },  
 "addressLine1": "RRRRRRRR",  
 "\_id": "b01d8cfb3f5a4854a0390b888bf52430"  
 }  
 ]  
 }  
 ]  
 }  
}

### Response with fields that are blank

The following search response contains the Person business entity record that doesn't contain a value for the Middle Name field, and the First Name field matches the specified search term:

{  
 "searchResult": {  
 "hits": 1,  
 "pageSize": 25,  
 "pageOffset": 0,  
 "recordsToReturn": 25,  
 "recordOffset": 0,  
 "maxRecords": 10000,  
 "records": [  
 {  
 "\_meta": {  
 "score": 1.0,  
 "updatedBy": "user1",  
 "createdBy": "user1",  
 "lastUpdateDate": "1725429200669",  
 "businessId": "MDM000004DLV9D",  
 "businessEntity": "c360.person",  
 "id": "MDM000004DLV9D",  
 "type": "c360.person",  
 "creationDate": "1725429200669",  
 "states": {  
 "recordStateIndicator": "ACTIVE"  
 },  
 "status": "ACTIVE"  
 },  
 "c360person.lastName": "Mark",  
 "c360person.firstName": "David"  
 }  
 ]  
 }  
}

### Response with fields that contain a value

The following search response contains the Person business entity record that has a Designation field, and the First Name field is equal to John.

{  
 "searchResult": {  
 "hits": 1,  
 "pageSize": 25,  
 "pageOffset": 0,  
 "recordsToReturn": 25,  
 "recordOffset": 0,  
 "maxRecords": 10000,  
 "records": [  
 {  
 "\_meta": {  
 "score": 1.0,  
 "updatedBy": "user1",  
 "createdBy": "user1",  
 "lastUpdateDate": "1715708126414",  
 "businessId": "MDM000004DGQ2O",  
 "businessEntity": "c360.person",  
 "id": "MDM000004DGQ2O",  
 "type": "c360.person",  
 "creationDate": "1715708124169",  
 "states": {  
 "recordStateIndicator": "ACTIVE"  
 },  
 "status": "ACTIVE"  
 },  
 "c360person.designation": "Doctor",  
 "c360person.lastName": "Joseph",  
 "c360person.firstName": "John"  
 }  
 ]  
 }  
}

### Response with records updated within a specific time period

The following response contains all the Person records that were updated within a specified time period:

{  
 "searchResult": {  
 "hits": 1027,  
 "pageSize": 25,  
 "pageOffset": 0,  
 "recordsToReturn": 25,  
 "recordOffset": 0,  
 "maxRecords": 10000,  
 "records": [  
 {  
 "\_meta": {  
 "score": 1.0,  
 "updatedBy": "Delta\_Jul2Adm",  
 "createdBy": "Delta\_Jul2Adm",  
 "lastUpdateDate": "1719987281992",  
 "businessId": "MDM00000000000",  
 "id": "MDM00000000000",  
 "businessEntity": "c360.person",  
 "type": "c360.person",  
 "creationDate": "1719987281992",  
 "states": {  
 "recordStateIndicator": "ACTIVE",  
 "validation": "PASSED"  
 },  
 "status": "ACTIVE"  
 },  
 "c360person.title": "xPzlPH",  
 "c360person.AlternateName": [  
 {  
 "\_id": "1",  
 "AlternateName": "gAdBOj"  
 }  
 ],  
 "c360person.birthPlace": "atjMFH",  
 "c360person.middleName": "FqZNng",  
 "c360person.prefixName": {  
 "Code": "Mr.",  
 "Name": "Mr."  
 },  
 "c360person.birthDate": "0020-01-01",  
 "c360person.designation": "rKYQYf",  
 "c360person.lastName": "jHqhMp",  
 "c360person.firstName": "Rajni\_1",  
 "c360person.gender": {  
 "Code": "M",  
 "Name": "Male"  
 },  
 "c360person.PostalAddress": [  
 {  
 "country": {  
 "Code": "US",  
 "Name": "United States"  
 },  
 "city": "NwEqOa",  
 "postalCode": "16361",  
 "county": "kKveYx",  
 "addressLine1": "jUHfYL",  
 "\_id": "1",  
 "state": {  
 "Code": "AK",  
 "Name": "Alaska"  
 }  
 }  
 ]  
 }  
   
 ]  
 }  
}

### Response with records based on dynamic field values

The following response contains the Product business entity records based on dynamic field values, such as product description, quantity, and electronics display size:

{  
 "status": "OK",  
 "startTime": "12:12:40:144",  
 "endTime": "12:12:40:159",  
 "timeTaken": "15ms",  
 "searchRequest": {  
 "tenantId": "xxxxxxxxxxxxxxxxxxxxxx",  
 "search": "ABC",  
 "source": [],  
 "entityType": "Product",  
 "recordsToReturn": 25,  
 "recordOffset": 0,  
 "pageSize": 25,  
 "pageOffset": 0,  
 "filters": {  
 "filter": [  
 {  
 "fieldName": "\_meta.states.base",  
 "fieldValue": [  
 "ACTIVE"  
 ],  
 "comparator": "IN"  
 }  
 ]  
 },  
 "facets": [  
 {  
 "type": "SIMPLE",  
 "facetField": "Product.product\_name",  
 "orderField": "KEY",  
 "order": "ASCENDING"  
 },  
 {  
 "type": "SIMPLE",  
 "facetField": "\_meta.states.base",  
 "orderField": "KEY",  
 "order": "ASCENDING"  
 },  
 {  
 "type": "SIMPLE",  
 "facetField": "\_cm.vader.message",  
 "orderField": "KEY",  
 "order": "ASCENDING"  
 },  
 {  
 "type": "SIMPLE",  
 "facetField": "\_meta.states.validation",  
 "orderField": "KEY",  
 "order": "ASCENDING"  
 }  
 ],  
 "sort": [],  
 "debug": false,  
 "highlight": true,  
 "hierarchyFilters": [],  
 "aggregateSelectedFacetFilterValues": false,  
 "includeDynamicFaceting": true,  
 "searchRequestType": "GENERIC\_SEARCH",  
 "filterDeleted": true,  
 "searchLocale": [  
 "en"  
 ],  
 "fetchSource": true,  
 "topNValue": 0  
 },  
 "searchResult": {  
 "hits": 3,  
 "pageSize": 25,  
 "pageOffset": 0,  
 "recordsToReturn": 25,  
 "recordOffset": 0,  
 "maxRecords": 10000,  
 "records": [  
 {  
 "Product.product\_code": "abc\_1",  
 "\_meta": {  
 "updatedBy": "voyagers",  
 "createdBy": "voyagers",  
 "lastUpdateDate": "1760594159120",  
 "businessId": "MDM00000003UW4",  
 "businessEntity": "Product",  
 "id": "68d4e5fa18a170c7cf26eae3",  
 "type": "Product",  
 "creationDate": "1758782970911",  
 "states": {  
 "validation": "PASSED",  
 "recordStateIndicator": "ACTIVE"  
 },  
 "status": "ACTIVE",  
 "score": 20.780094  
 },  
 "highlight": {  
 "Product.product\_code": [  
 "<em>abc\_1</em>"  
 ],  
 "Product.product\_name": [  
 "<em>ABC</em>"  
 ]  
 },  
 "Product.product\_name": "ABC",  
 "Product.Product\_dfv": {  
 "amount": "4",  
 "electronics": [  
 {  
 "display\_size": 536,  
 "\_id": "3e9c8a4dc4644b1aa4377852d9c72427",  
 "connectivity\_type": "65"  
 }  
 ],  
 "product\_description": "ABC gives a seamless smart TV experience",  
 "quntity": 3  
 }  
 },  
 {  
 "Product.product\_code": "3434",  
 "\_meta": {  
 "updatedBy": "voyagers",  
 "createdBy": "voyagers",  
 "lastUpdateDate": "1761872326244",  
 "businessId": "MDM00000006FHC",  
 "businessEntity": "Product",  
 "id": "6904093bf294de1941e7b321",  
 "type": "Product",  
 "creationDate": "1761872187671",  
 "states": {  
 "validation": "PASSED",  
 "recordStateIndicator": "ACTIVE"  
 },  
 "status": "ACTIVE",  
 "score": 20.780094  
 },  
 "highlight": {  
 "Product.product\_name": [  
 "<em>ABC</em>"  
 ]  
 },  
 "Product.product\_name": "ABC",  
 "Product.Product\_dfv": {  
 "amount": "4545454",  
 "product\_description": "ABC gives a seamless smart TV experience",  
 "quntity": 55  
 }  
 },  
 {  
 "Product.product\_code": "S001",  
 "\_meta": {  
 "updatedBy": "voyagers",  
 "createdBy": "voyagers",  
 "lastUpdateDate": "1762929015177",  
 "businessId": "MDM00000004ABK",  
 "businessEntity": "Product",  
 "id": "68e5f1cd55ebb333754ebaf2",  
 "type": "Product",  
 "creationDate": "1759900109492",  
 "states": {  
 "validation": "PASSED",  
 "recordStateIndicator": "ACTIVE"  
 },  
 "status": "ACTIVE",  
 "score": 11.712665  
 },  
 "highlight": {  
 "Product.product\_name": [  
 "<em>ABC</em> TV"  
 ]  
 },  
 "Product.product\_name": "ABC TV",  
 "Product.Product\_dfv": {  
 "amount": "45635",  
 "electronics": [  
 {  
 "os\_specification": [  
 {  
 "os\_type": "Android",  
 "\_id": "530a815d0a204a9fa44d20dea01d763f"  
 }  
 ],  
 "display\_size": 55,  
 "\_id": "dd6ecd8ab194411bacbd34810efa03ed"  
 }  
 ],  
 "quantity": 100  
 }  
 }  
 ],  
 "facetResults": [  
 {  
 "facetField": "Product.Product\_dfv.quantity",  
 "type": "SIMPLE",  
 "facetTerms": [  
 {  
 "key": "3",  
 "count": 1  
 },  
 {  
 "key": "55",  
 "count": 1  
 },  
 {  
 "key": "100",  
 "count": 1  
 }  
 ],  
 "totalCount": 3  
 },  
 {  
 "facetField": "Product.Product\_dfv.electronics.os\_specification.os\_type",  
 "type": "SIMPLE",  
 "facetTerms": [  
 {  
 "key": "android",  
 "count": 1  
 }  
 ],  
 "totalCount": 1  
 },  
 {  
 "facetField": "Product.Product\_dfv.electronics.connectivity\_type",  
 "type": "SIMPLE",  
 "facetTerms": [  
 {  
 "key": "65",  
 "count": 1  
 }  
 ],  
 "totalCount": 1  
 },  
 {  
 "facetField": "Product.Product\_dfv.electronics.display\_size",  
 "type": "SIMPLE",  
 "facetTerms": [  
 {  
 "key": "55",  
 "count": 1  
 },  
 {  
 "key": "536",  
 "count": 1  
 }  
 ],  
 "totalCount": 2  
 },  
 {  
 "facetField": "\_meta.states.base",  
 "type": "SIMPLE",  
 "facetTerms": [  
 {  
 "key": "ACTIVE",  
 "count": 3  
 }  
 ],  
 "totalCount": 3  
 },  
 {  
 "facetField": "Product.product\_name",  
 "type": "SIMPLE",  
 "facetTerms": [  
 {  
 "key": "abc",  
 "count": 2  
 },  
 {  
 "key": "abc tv",  
 "count": 1  
 }  
 ],  
 "totalCount": 3  
 },  
 {  
 "facetField": "Product.Product\_dfv.product\_description",  
 "type": "SIMPLE",  
 "facetTerms": [  
 {  
 "key": "abc gives a seamless smart TV experience",  
 "count": 2  
 }  
 ],  
 "totalCount": 2  
 },  
 {  
 "facetField": "Product.Product\_dfv.amount",  
 "type": "RANGE",  
 "facetTerms": [  
 {  
 "key": "4.0 - 4545454.0",  
 "count": 3  
 }  
 ],  
 "totalCount": 3  
 },  
 {  
 "facetField": "\_cm.vader.message",  
 "type": "SIMPLE",  
 "facetTerms": [],  
 "validationBreakDown": {},  
 "totalCount": 0  
 },  
 {  
 "facetField": "\_meta.states.validation",  
 "type": "SIMPLE",  
 "facetTerms": [  
 {  
 "key": "PASSED",  
 "count": 3  
 }  
 ],  
 "totalCount": 3  
 }  
 ],  
 "dynamicFacetFields": {  
 "topNValue": 25,  
 "totalFacets": 6,  
 "topFacetFields": [  
 {  
 "key": "Product.Product\_dfv.amount",  
 "count": 3  
 },  
 {  
 "key": "Product.Product\_dfv.quntity",  
 "count": 3  
 },  
 {  
 "key": "Product.Product\_dfv.product\_description",  
 "count": 2  
 },  
 {  
 "key": "Product.Product\_dfv.electronics.display\_size",  
 "count": 2  
 },  
 {  
 "key": "Product.Product\_dfv.electronics.os\_specification.os\_type",  
 "count": 1  
 },  
 {  
 "key": "Product.Product\_dfv.electronics.connectivity\_type",  
 "count": 1  
 }  
 ]  
 }  
 }  
}

### Response with search results that exceed the dynamic field index limit

The following search response contains Keyboard record in the Item business entity that exceeds the dynamic field index limit:

{  
 "searchResult": {  
 "hits": 1,  
 "pageSize": 0,  
 "pageOffset": 0,  
 "recordsToReturn": 0,  
 "recordOffset": 0,  
 "maxRecords": 10000,  
 "records": [  
 {  
 "\_meta": {  
 "updatedBy": "user1",  
 "createdBy": "user1",  
 "lastUpdateDate": "1783084483210",  
 "businessId": "MDM0000000003C",  
 "id": "MDM0000000003C",  
 "businessEntity": "Item",  
 "type": "Item",  
 "creationDate": "1782441105026",  
 "states": {  
 "validation": "PASSED",  
 "recordStateIndicator": "ACTIVE"  
 },  
 "status": "ACTIVE",  
 "score": 6.1987  
 },  
 "**\_indexValidationError**": [  
 {  
 "code": "**INDEX.100001**"  
 }  
 ],  
 "Item.item\_dfv": {  
 "release\_date": "2026-06-01",  
 "fg": [  
 {  
 "name": "Keyboard",  
 "warranty\_months": 24,  
 "weight\_kg": 1.0  
 }  
 ],  
 "unit\_count": 1,  
 "in\_stock": true,  
 "launch\_timestamp": 1782824430000,  
 "rating": 1.0,  
 "description": "Keyboard",  
 "price": "1"  
 },  
 "Item.item\_name": "Keyboard",  
 "Item.item\_code": 1  
 }  
 ]  
 }  
}

### Response with search results that exceed the field group entry index limit

The following search response contains James Patel record in the c360.person business entity that exceeds the field group entry index limit:

{  
 "searchResult": {  
 "hits": 1,  
 "pageSize": 0,  
 "pageOffset": 0,  
 "recordsToReturn": 0,  
 "recordOffset": 0,  
 "maxRecords": 10000,  
 "records": [  
 {  
 "c360person.firstName": "James",  
 "c360person.lastName": "Patel",  
 "\_meta": {  
 "updatedBy": "user1",  
 "createdBy": "user1",  
 "lastUpdateDate": "1781202587702",  
 "businessId": "MDM0000000003W",  
 "id": "MDM0000000003W",  
 "businessEntity": "c360.person",  
 "type": "c360.person",  
 "creationDate": "1778571050125",  
 "states": {  
 "validation": "PASSED",  
 "recordStateIndicator": "ACTIVE"  
 },  
 "status": "ACTIVE",  
 "score": 10.962221  
 },  
 "**\_indexValidationError**": [  
 {  
 "code": "**INDEX.100002**"  
 }  
 ],  
 "c360person.designation": "Director",  
 "c360person.dfv\_c360person": {  
 "name": "James Patel",  
 "city": "San Jose",  
 "postalCode": "95111"   
 }  
 }  
 ]  
 }  
}

## Filtering and sorting search results

When you submit a search request, you can specify how to filter and search the search results.

The following table lists the predefined fields you can use to filter and sort the search results:

| Business Entity | Filter by | Sort by |
| --- | --- | --- |
| Category | * \_cm.vader.message * \_meta.states.validation * p360category.keywords.keyword | * p360category.descriptions.name * p360category.descriptions.description * p360category.descriptions.descriptionsLanguage.Name * p360category.identifier * p360category.keywords.keyword * p360category.keywords.keywordsLanguage.Name |
| Contact | * \_cm.vader.message * \_meta.states.validation * s360Contact.gender.Name * s360Contact.PostalAddress.country.Name * s360Contact.PostalAddress.state.Name * s360Contact.PostalAddress.city | * s360Contact.firstName * s360Contact.lastName * s360Contact.gender.Name * s360Contact.jobTitle.Name * s360Contact.department.Name * s360Contact.language.Name * s360Contact.PostalAddress.country.Name * s360Contact.PostalAddress.state.Name * s360Contact.PostalAddress.city * s360Contact.PostalAddress.postalCode * s360Contact.PostalAddress.addressLine1 * s360Contact.PhoneCommunication.phoneNumber * s360Contact.ElectronicAddressCommunication.electronicAddress |
| Item | * \_cm.vader.message * \_meta.states.validation * p360item.availabilityDate * p360item.availability.Name * p360item.brand * p360item.countryOfOrigin.Name * p360item.expirationDate * p360item.keywords.keyword * p360item.manufacturer * p360item.metaInformation.metaDescription * p360item.status.Name * p360item.sellingPrice.sellingValidTo * p360item.sellingPrice.sellingValidFrom * p360item.sellingPrice.sellingCurrency.Name * p360item.sellingPrice.sellingTerritory.Name * p360item.sellingPrice.sellingPriceType.Name * p360item.sellingPrice.sellingPriceAmount * p360item.subBrand * p360item.tags.tag | * p360item.availabilityDate * p360item.availability.Name * p360item.brand * p360item.countryOfOrigin.Name * p360item.description.longDescription * p360item.description.productTitle * p360item.description.shortDescription * p360item.expirationDate * p360item.gtin * p360item.identifier * p360item.keywords.keyword * p360item.manufacturer * p360item.manufacturerProductNumber * p360item.manufacturerGLN * p360item.metaInformation.metaDescription * p360item.metaInformation.metaTitle * p360item.productHighlights.productHighlight * p360item.status.Name * p360item.sellingPrice.sellingValidTo * p360item.sellingPrice.sellingValidFrom * p360item.sellingPrice.sellingCurrency.Name * p360item.sellingPrice.sellingTerritory.Name * p360item.sellingPrice.sellingPriceType.Name * p360item.sellingPrice.sellingPriceAmount * p360item.subBrand * p360item.tags.tag |
| Organization | * \_cm.vader.message * \_meta.states.validation * c360organization.companyType.Name * c360organization.PostalAddress.city * c360organization.PostalAddress.country.Name * c360organization.PostalAddress.state.Name | * c360organization.name * c360organization.companyType.Name * c360organization.PostalAddress.addressLine1 * c360organization.PostalAddress.country.Name * c360organization.PostalAddress.state.Name * c360organization.PostalAddress.city * c360organization.PostalAddress.county * c360organization.PostalAddress.postalCode * c360organization.PhoneCommunication.phoneNumber * c360organization.AlternateName.name * c360organization.Segment.segmentType.Name |
| Person | * \_cm.vader.message * \_meta.states.validation * c360person.gender.Name * c360person.PostalAddress.city * c360person.PostalAddress.country.Name * c360person.PostalAddress.state.Name | * c360person.firstName * c360person.lastName * c360person.designation * c360person.gender.Name * c360person.AlternateName.AlternateName * c360person.PostalAddress.addressLine1 * c360person.PostalAddress.city * c360person.PostalAddress.county * c360person.PostalAddress.country.Name * c360person.PostalAddress.state.Name * c360person.PostalAddress.postalCode * c360person.Phone.phoneNumber |
| Supplier | * \_cm.vader.message * \_meta.states.validation * s360Supplier.supplierType * s360SupplierPostalAddress.country.Name * s360SupplierPostalAddress.state.Name * s360SupplierPostalAddress.city | * s360Supplier.name * s360Supplier.dunsNumber * s360Supplier.supplierType * s360SupplierPostalAddress.country.Name * s360SupplierPostalAddress.state.Name * s360SupplierPostalAddress.city * s360SupplierPostalAddress.postalCode * s360SupplierPostalAddress.addressLine1 * s360Supplier.PhoneCommunication.phoneNumber * s360Supplier.Diversity.diversityStatus * s360Supplier.Diversity.diversityType * s360Supplier.SourcingManager.sourcingManagerName * s360Supplier.SourcingManager.sourcingManagerEmployeeId * s360Supplier.Segment.segmentType * s360Supplier.Segment.segmentName * s360Supplier.ElectronicAddressCommunication.electronicAddress |

**Note:** 

Effective in the May 2023 release, \_cm.vader.message and \_meta.states.validation fields are available for preview.

Preview functionality is supported for evaluation purposes but is unwarranted and is not supported in production environments or any environment that you plan to push to production. Informatica intends to include the preview functionality in an upcoming release for production use, but might choose not to in accordance with changing market or technical circumstances. For more information, contact Informatica Global Customer Support.

## Filtering records that exceed the index limit

When the generate search index job runs, it indexes a limited number of dynamic fields and field group entries for each record. If a record exceeds a limit, the job indexes only the values within the limit and adds an error code to the record.

The generate search index job applies the following index limits:

Dynamic field index limit
:   The job indexes a maximum of 100 dynamic fields for each record. If a record contains more than 100 dynamic fields, the job indexes only the first 100 fields. When you search for records based on dynamic fields, the search returns results only for the indexed fields.

For each record that exceeds the dynamic field limit, the job adds the error code INDEX.100001 to the record in the \_indexValidationError.code system field.

Field group entry index limit
:   The job indexes a maximum of 25,000 field group and nested field group entries for each record. If a record contains more than 25,000 entries, the job indexes only the first 25,000 entries. When you search for records, the search returns results only for the indexed entries.

The 25,000 entry limit includes field group entries and dynamic fields. Field group entries also include validation errors, which MDM SaaS generates when a record doesn't satisfy a data quality rule. When the job indexes records, validation errors take precedence over other field group entries and dynamic fields. For example, a record contains 20,000 validation errors and 6,000 address field group entries. The job indexes all 20,000 validation errors and only 5,000 of the 6,000 address field group entries.

For each record that exceeds the field group entry index limit, the job adds the error code INDEX.100002 to the record in the \_indexValidationError.code system field.

**Note:** 

Effective in the October 2026 release, the ability to filter records that exceed the field group entry index limit is available for preview.

Preview functionality is supported for evaluation purposes but is unwarranted and is not supported in production environments or any environment that you plan to push to production. Informatica intends to include the preview functionality in an upcoming release for production use, but might choose not to in accordance with changing market or technical circumstances. For more information, contact Informatica Global Customer Support.

### Filter records based on the error code

You can filter the search results on the \_indexValidationError.code system field to retrieve only the records that exceed a limit.

The following table lists the filter parameters to use:

| Parameter | System Field | Search Parameter Value | Description |
| --- | --- | --- | --- |
| fieldName > fieldValue | \_indexValidationError.code | INDEX.100001 | Filters search results to retrieve records that exceed the dynamic field index limit. |
| fieldName > fieldValue | \_indexValidationError.code | INDEX.100002 | Filters search results to retrieve records that exceed the field group entry index limit. |

You can use only the following filter comparators to search for records with fields that contain the error code:

* NOT\_EQUALS
* IN
* IS\_MISSING
* IS\_NOT\_MISSING

**Note:** 

To apply these changes, reindex the records in your business entities. To reindex the records, run the Generate Search and Report Indexes or Match Keys job by using the MDMSearchIndex process.

For more information, see [filter records that exceed the dynamic field index limit](Search_API.md#ww3_6_43_14_2_8_1 "Search API") and [filter records that exceed the field group entry index limit](Search_API.md#ww3_6_43_14_2_9_1 "Search API").